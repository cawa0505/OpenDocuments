use std::sync::Arc;
// removed unused std::collections::HashMap
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
use crate::McpState;
use crate::utils::resolve_workspace_id;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamRequest {
    pub query: String,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default, rename = "conversationId")]
    pub conversation_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub query: String,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default, rename = "conversationId")]
    pub conversation_id: Option<String>,
}

// ── Helper to fetch and format recent conversation messages for RAG context ──
async fn get_history_context(db_pool: &sqlx::SqlitePool, conversation_id: &str) -> String {
    let rows_res = sqlx::query(
        "SELECT role, content FROM messages WHERE conversation_id = ? ORDER BY created_at DESC LIMIT 6"
    )
    .bind(conversation_id)
    .fetch_all(db_pool)
    .await;

    match rows_res {
        Ok(rows) => {
            let mut formatted = Vec::new();
            for r in rows.into_iter().rev() {
                let role: String = sqlx::Row::get(&r, 0);
                let content: String = sqlx::Row::get(&r, 1);
                let display_role = if role.to_lowercase() == "user" { "User" } else { "Assistant" };
                formatted.push(format!("{}: {}", display_role, content));
            }
            formatted.join("\n")
        }
        Err(e) => {
            eprintln!("💥 讀取對話歷史失敗: {e}");
            String::new()
        }
    }
}

// ── BYOK LLM Helpers ──
async fn get_active_llm_client(db_pool: &sqlx::SqlitePool, workspace_id: &str) -> Option<opendoc_llm::LlmClient> {
    let row_res = sqlx::query(
        "SELECT name, provider, base_url, model, api_key FROM llm_providers WHERE workspace_id = ? AND is_active = 1 LIMIT 1"
    )
    .bind(workspace_id)
    .fetch_optional(db_pool)
    .await;

    match row_res {
        Ok(Some(row)) => {
            let name: String = sqlx::Row::get(&row, 0);
            let _provider_name: String = sqlx::Row::get(&row, 1);
            let base_url: String = sqlx::Row::get(&row, 2);
            let model: String = sqlx::Row::get(&row, 3);
            let api_key: String = sqlx::Row::get(&row, 4);

            let api_key_opt = if api_key.is_empty() { None } else { Some(api_key) };

            let provider = opendoc_llm::LlmProvider {
                name,
                base_url,
                model,
                api_key: api_key_opt,
            };
            Some(opendoc_llm::LlmClient::new(provider))
        }
        _ => None,
    }
}

async fn get_history_messages(db_pool: &sqlx::SqlitePool, conversation_id: &str) -> Vec<opendoc_llm::ChatMessage> {
    let rows_res = sqlx::query(
        "SELECT role, content FROM messages WHERE conversation_id = ? ORDER BY created_at DESC LIMIT 6"
    )
    .bind(conversation_id)
    .fetch_all(db_pool)
    .await;

    match rows_res {
        Ok(rows) => {
            let mut msgs = Vec::new();
            for r in rows.into_iter().rev() {
                let role: String = sqlx::Row::get(&r, 0);
                let content: String = sqlx::Row::get(&r, 1);
                if role.to_lowercase() == "user" {
                    msgs.push(opendoc_llm::ChatMessage::user(content));
                } else if role.to_lowercase() == "assistant" {
                    msgs.push(opendoc_llm::ChatMessage::assistant(content));
                }
            }
            msgs
        }
        Err(e) => {
            eprintln!("💥 讀取對話歷史強型別失敗: {e}");
            Vec::new()
        }
    }
}

pub async fn chat_stream_handler(
    State(state): State<Arc<McpState>>,
    headers: HeaderMap,
    Json(req): Json<ChatStreamRequest>,
) -> Result<Sse<std::pin::Pin<Box<dyn Stream<Item = Result<Event, std::convert::Infallible>> + Send>>>, (StatusCode, Json<Value>)> {
    use futures_util::StreamExt;

    let workspace = match resolve_workspace_id(&state, &headers).await {
        Ok(w) => w,
        Err(e) => {
            eprintln!("💥 取得 workspace_id 失敗: {e}");
            return Err((e, Json(json!({ "error": "invalid workspace" }))));
        }
    };
    let query_id = Uuid::new_v4().to_string();
    let profile = req.profile.unwrap_or_else(|| "balanced".to_string());
    let mut conversation_id = req.conversation_id;

    if let Some(cid) = &conversation_id {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
        )
        .bind(cid)
        .bind(&workspace)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Internal server error" })),
            )
        })?;

        if count == 0 {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            ));
        }
    } else {
        let new_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO conversations (id, workspace_id, title, created_at, updated_at) VALUES (?, ?, '新對話', datetime('now'), datetime('now'))")
            .bind(&new_id)
            .bind(&workspace)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                eprintln!("💥 自動建立 conversation 失敗: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Failed to auto-create conversation" })),
                )
            })?;
        conversation_id = Some(new_id);
    }
    let threshold = match profile.as_str() {
        "fast" => 0.50,
        "precise" => 0.70,
        _ => 0.60,
    };

    let mut expanded_query = req.query.clone();
    if let Some(cid) = &conversation_id {
        let history = get_history_context(&state.db_pool, cid).await;
        if !history.is_empty() {
            expanded_query = format!("{}\n\n[Recent Conversation History]\n{}", req.query, history);
        }
    }

    let results = state.search.search_and_rerank(&expanded_query, threshold);
    let limited: Vec<_> = results.into_iter().take(10).collect();

    let answer = if limited.is_empty() {
        "在現有文獻中未找到相關內容。".to_string()
    } else {
        limited.iter().enumerate().map(|(i, r)| {
            format!("[{}] {}", i + 1, r.content.chars().take(500).collect::<String>())
        }).collect::<Vec<_>>().join("\n\n")
    };

    let total_score: f32 = if limited.is_empty() { 0.0 }
        else { limited.iter().map(|r| r.relevance_score.unwrap_or(0.0)).sum::<f32>() / limited.len() as f32 };
    let (level, reason) = if total_score >= 0.75 { ("high", "多個高相關性片段") }
        else if total_score >= 0.55 { ("medium", "找到部分相關內容") }
        else if total_score >= 0.35 { ("low", "僅找到少量模糊匹配") }
        else { ("none", "未找到明確相關內容") };

    let sources_mapped: Vec<Value> = limited.iter().map(|r| {
        let source_path = r.metadata.get("source_path")
            .and_then(|v| v.as_str())
            .unwrap_or(&r.file_path);
        json!({
            "chunkId": format!("{}:{}", r.file_path, r.content.len()),
            "content": r.content,
            "score": r.relevance_score.unwrap_or(0.0),
            "documentId": r.file_path,
            "chunkType": format!("{:?}", r.chunk_type),
            "headingHierarchy": [],
            "sourcePath": source_path,
            "sourceType": "file",
        })
    }).collect();
    let sources_json = serde_json::to_string(&sources_mapped).unwrap_or_else(|_| "[]".to_string());
    let confidence_json = serde_json::to_string(&json!({
        "score": total_score, "level": level, "reason": reason
    })).unwrap_or_default();

    let locale_str = headers.get("x-locale")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("zh-TW");

    let language_instruction = match locale_str {
        "en" => "English",
        "ko" => "Korean (한국어)",
        _ => "Traditional Chinese (繁體中文)",
    };

    let llm_client_opt = get_active_llm_client(&state.db_pool, &workspace).await;

    if let Some(llm_client) = llm_client_opt {
        let sources_json_clone = sources_json.clone();
        let confidence_json_clone = confidence_json.clone();

        let context_str = if limited.is_empty() {
            match locale_str {
                "en" => "No relevant local documents found. Please answer directly or explain that there is no context.".to_string(),
                "ko" => "관련된 현지 문서를 찾을 수 없습니다. 직접 답변하거나 관련 문서가 없음을 설명하십시오.".to_string(),
                _ => "未找到相關本地文獻。請直接回答使用者的問題，或說明沒有相關文獻。".to_string(),
            }
        } else {
            limited.iter().enumerate().map(|(i, r)| {
                format!("[Document {}] Source: {}\nContent: {}", i + 1, r.file_path, r.content)
            }).collect::<Vec<_>>().join("\n\n")
        };

        let system_prompt = format!(
            "You are a professional local knowledge base assistant. Answer the user's question based on the provided [Local Documents] below.\n\
             You MUST follow these rules:\n\
             1. You MUST answer in the requested language: {}.\n\
             2. When your answer is derived from or references a specific [Local Document] chunk, you MUST precisely append its corresponding citation tag at the end of the sentence, such as `[1]` or `[2]` (using single-byte square brackets and numbers). Never invent non-existent citation numbers.\n\
             3. If the documents do not contain relevant information, honestly state that the documents do not mention it, do not make up facts.\n\n\
             [Local Documents]\n{}",
             language_instruction,
             context_str
        );

        let cid_clone = conversation_id.clone();
        let query_clone = req.query.clone();
        let profile_clone = profile.clone();
        let workspace_clone = workspace.clone();
        let query_id_clone = query_id.clone();
        let state_clone = state.clone();
        let sources_mapped_clone = sources_mapped.clone();

        let mut messages = if let Some(cid) = &conversation_id {
            get_history_messages(&state.db_pool, cid).await
        } else {
            Vec::new()
        };
        messages.push(opendoc_llm::ChatMessage::user(&req.query));

        let opts = opendoc_llm::CompletionOptions {
            temperature: Some(0.3),
            max_tokens: None,
            system_prompt: Some(system_prompt),
        };

        let llm_stream_res = llm_client.stream(messages, &opts).await;

        match llm_stream_res {
            Ok(mut llm_stream) => {
                let s = async_stream::stream! {
                    yield Ok::<Event, std::convert::Infallible>(Event::default().event("sources").data(sources_json_clone));
                    yield Ok::<Event, std::convert::Infallible>(Event::default().event("confidence").data(confidence_json_clone));

                    let mut full_answer = String::new();

                    while let Some(res) = llm_stream.next().await {
                        match res {
                            Ok(token) => {
                                full_answer.push_str(&token);
                                let chunk_data = serde_json::to_string(&token).unwrap_or_default();
                                yield Ok::<Event, std::convert::Infallible>(Event::default().event("chunk").data(chunk_data));
                            }
                            Err(e) => {
                                eprintln!("💥 LLM stream error: {e}");
                                let error_data = serde_json::to_string(&json!({ "error": e.to_string() })).unwrap_or_default();
                                yield Ok::<Event, std::convert::Infallible>(Event::default().event("error").data(error_data));
                                break;
                            }
                        }
                    }

                    if let Some(cid) = &cid_clone {
                        let user_msg_id = Uuid::new_v4().to_string();
                        let assistant_msg_id = Uuid::new_v4().to_string();
                        let sources_str = serde_json::to_string(&sources_mapped_clone).unwrap_or_default();

                        let db = &state_clone.db_pool;

                        if let Err(e) = sqlx::query(
                            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, 'user', ?, datetime('now'))"
                        )
                        .bind(&user_msg_id)
                        .bind(cid)
                        .bind(&query_clone)
                        .execute(db)
                        .await {
                            eprintln!("💥 串流寫入 user messages 失敗: {e}");
                        }

                        if let Err(e) = sqlx::query(
                            "INSERT INTO messages (id, conversation_id, role, content, sources, profile_used, confidence_score, response_time_ms, created_at) VALUES (?, ?, 'assistant', ?, ?, ?, ?, ?, datetime('now'))"
                        )
                        .bind(&assistant_msg_id)
                        .bind(cid)
                        .bind(&full_answer)
                        .bind(&sources_str)
                        .bind(&profile_clone)
                        .bind(total_score)
                        .bind(100i64)
                        .execute(db)
                        .await {
                            eprintln!("💥 串流寫入 assistant messages 失敗: {e}");
                        }
                    }

                    if let Err(e) = sqlx::query(
                        "INSERT INTO query_logs (query, profile, confidence_score, response_time_ms, route, workspace_id) VALUES (?, ?, ?, ?, 'rag', ?)"
                    )
                    .bind(&query_clone)
                    .bind(&profile_clone)
                    .bind(total_score)
                    .bind(100i64)
                    .bind(&workspace_clone)
                    .execute(&state_clone.db_pool)
                    .await {
                        eprintln!("💥 串流寫入 query_logs 失敗: {e}");
                    }

                    let done_json = serde_json::to_string(&json!({
                        "queryId": query_id_clone, "route": "rag", "profile": profile_clone,
                        "conversationId": cid_clone,
                    })).unwrap_or_default();
                    yield Ok::<Event, std::convert::Infallible>(Event::default().event("done").data(done_json));
                };

                return Ok(Sse::new(Box::pin(s) as std::pin::Pin<Box<dyn Stream<Item = Result<Event, std::convert::Infallible>> + Send>>).keep_alive(KeepAlive::default()));
            }
            Err(e) => {
                eprintln!("💥 貼身 LLM Stream 失敗，Fallback 到 Echo 模式: {e}");
            }
        }
    }

    // Fallback stream (concatenated chunks)
    let s_fallback = async_stream::stream! {
        yield Ok::<Event, std::convert::Infallible>(Event::default().event("sources").data(sources_json));
        yield Ok::<Event, std::convert::Infallible>(Event::default().event("confidence").data(confidence_json));
        yield Ok::<Event, std::convert::Infallible>(Event::default().event("chunk").data(serde_json::to_string(&answer).unwrap_or_default()));

        if let Some(cid) = &conversation_id {
            let user_msg_id = Uuid::new_v4().to_string();
            let assistant_msg_id = Uuid::new_v4().to_string();
            let sources_str = serde_json::to_string(&sources_mapped).unwrap_or_default();

            if let Err(e) = sqlx::query(
                "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, 'user', ?, datetime('now'))"
            )
            .bind(&user_msg_id)
            .bind(cid)
            .bind(&req.query)
            .execute(&state.db_pool)
            .await {
                eprintln!("💥 Fallback 寫入 user messages 失敗: {e}");
            }

            if let Err(e) = sqlx::query(
                "INSERT INTO messages (id, conversation_id, role, content, sources, profile_used, confidence_score, response_time_ms, created_at) VALUES (?, ?, 'assistant', ?, ?, ?, ?, ?, datetime('now'))"
            )
            .bind(&assistant_msg_id)
            .bind(cid)
            .bind(&answer)
            .bind(&sources_str)
            .bind(&profile)
            .bind(total_score)
            .bind(100i64)
            .execute(&state.db_pool)
            .await {
                eprintln!("💥 Fallback 寫入 assistant messages 失敗: {e}");
            }
        }

        if let Err(e) = sqlx::query(
            "INSERT INTO query_logs (query, profile, confidence_score, response_time_ms, route, workspace_id) VALUES (?, ?, ?, ?, 'rag', ?)"
        )
        .bind(&req.query)
        .bind(&profile)
        .bind(total_score)
        .bind(100i64)
        .bind(&workspace)
        .execute(&state.db_pool)
        .await {
            eprintln!("💥 Fallback 寫入 query_logs 失敗: {e}");
        }

        let done_json = serde_json::to_string(&json!({
            "queryId": query_id, "route": "rag", "profile": profile,
            "conversationId": conversation_id,
        })).unwrap_or_default();
        yield Ok::<Event, std::convert::Infallible>(Event::default().event("done").data(done_json));
    };

    Ok(Sse::new(Box::pin(s_fallback) as std::pin::Pin<Box<dyn Stream<Item = Result<Event, std::convert::Infallible>> + Send>>).keep_alive(KeepAlive::default()))
}

pub async fn chat_handler(
    State(state): State<Arc<McpState>>,
    headers: HeaderMap,
    Json(req): Json<ChatRequest>,
) -> Result<Json<Value>, StatusCode> {
    let workspace = resolve_workspace_id(&state, &headers).await? ;
    let query_id = Uuid::new_v4().to_string();
    let profile = req.profile.unwrap_or_else(|| "balanced".to_string());
    let conversation_id = req.conversation_id;

    if let Some(cid) = &conversation_id {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
        )
        .bind(cid)
        .bind(&workspace)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢 conversation 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if count == 0 {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    let threshold = match profile.as_str() {
        "fast" => 0.50,
        "precise" => 0.70,
        _ => 0.60,
    };

    let mut expanded_query = req.query.clone();
    if let Some(cid) = &conversation_id {
        let history = get_history_context(&state.db_pool, cid).await;
        if !history.is_empty() {
            expanded_query = format!("{}\n\n[Recent Conversation History]\n{}", req.query, history);
        }
    }

    let results = state.search.search_and_rerank(&expanded_query, threshold);
    let limited: Vec<_> = results.into_iter().take(10).collect();

    let locale_str = headers.get("x-locale")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("zh-TW");

    let language_instruction = match locale_str {
        "en" => "English",
        "ko" => "Korean (한국어)",
        _ => "Traditional Chinese (繁體中文)",
    };

    let mut answer = String::new();
    let mut generated_by_llm = false;

    if let Some(llm_client) = get_active_llm_client(&state.db_pool, &workspace).await {
        let context_str = if limited.is_empty() {
            match locale_str {
                "en" => "No relevant local documents found. Please answer directly or explain that there is no context.".to_string(),
                "ko" => "관련된 현지 문서를 찾을 수 없습니다. 직접 답변하거나 관련 문서가 없음을 설명하십시오.".to_string(),
                _ => "未找到相關本地文獻。請直接回答使用者的問題，或說明沒有相關文獻。".to_string(),
            }
        } else {
            limited.iter().enumerate().map(|(i, r)| {
                format!("[Document {}] Source: {}\nContent: {}", i + 1, r.file_path, r.content)
            }).collect::<Vec<_>>().join("\n\n")
        };

        let system_prompt = format!(
            "You are a professional local knowledge base assistant. Answer the user's question based on the provided [Local Documents] below.\n\
             You MUST follow these rules:\n\
             1. You MUST answer in the requested language: {}.\n\
             2. When your answer is derived from or references a specific [Local Document] chunk, you MUST precisely append its corresponding citation tag at the end of the sentence, such as `[1]` or `[2]` (using single-byte square brackets and numbers). Never invent non-existent citation numbers.\n\
             3. If the documents do not contain relevant information, honestly state that the documents do not mention it, do not make up facts.\n\n\
             [Local Documents]\n{}",
             language_instruction,
             context_str
        );

        let mut messages = if let Some(cid) = &conversation_id {
            get_history_messages(&state.db_pool, cid).await
        } else {
            Vec::new()
        };
        messages.push(opendoc_llm::ChatMessage::user(&req.query));

        let opts = opendoc_llm::CompletionOptions {
            temperature: Some(0.3),
            max_tokens: None,
            system_prompt: Some(system_prompt),
        };

        match llm_client.complete(messages, &opts).await {
            Ok(generated) => {
                answer = generated;
                generated_by_llm = true;
            }
            Err(e) => {
                eprintln!("💥 LLM 生成失敗，自動 Fallback 至 Echo 模式: {e}");
            }
        }
    }

    if !generated_by_llm {
        answer = if limited.is_empty() {
            "在現有文獻中未找到相關內容。".to_string()
        } else {
            limited.iter().enumerate().map(|(i, r)| {
                format!("[{}] {}", i + 1, r.content.chars().take(500).collect::<String>())
            }).collect::<Vec<_>>().join("\n\n")
        };
    }

    let total_score: f32 = if limited.is_empty() { 0.0 }
        else { limited.iter().map(|r| r.relevance_score.unwrap_or(0.0)).sum::<f32>() / limited.len() as f32 };
    let (level, reason) = if total_score >= 0.75 { ("high", "多個高相關性片段") }
        else if total_score >= 0.55 { ("medium", "找到部分相關內容") }
        else if total_score >= 0.35 { ("low", "僅找到少量模糊匹配") }
        else { ("none", "未找到明確相關內容") };

    let sources_mapped: Vec<Value> = limited.iter().map(|r| {
        let source_path = r.metadata.get("source_path")
            .and_then(|v| v.as_str())
            .unwrap_or(&r.file_path);
        json!({
            "chunkId": format!("{}:{}", r.file_path, r.content.len()),
            "content": r.content,
            "score": r.relevance_score.unwrap_or(0.0),
            "documentId": r.file_path,
            "chunkType": format!("{:?}", r.chunk_type),
            "headingHierarchy": [],
            "sourcePath": source_path,
            "sourceType": "file",
        })
    }).collect();

    let confidence_json = json!({
        "score": total_score, "level": level, "reason": reason
    });

    if let Err(e) = sqlx::query(
        "INSERT INTO query_logs (query, profile, confidence_score, response_time_ms, route, workspace_id) VALUES (?, ?, ?, ?, 'rag', ?)"
    )
    .bind(&req.query)
    .bind(&profile)
    .bind(total_score)
    .bind(100i64)
    .bind(&workspace)
    .execute(&state.db_pool)
    .await {
        eprintln!("💥 寫入 query_logs 失敗: {e}");
    }

    if let Some(cid) = &conversation_id {
        let user_msg_id = Uuid::new_v4().to_string();
        let assistant_msg_id = Uuid::new_v4().to_string();
        let sources_str = serde_json::to_string(&sources_mapped).unwrap_or_default();

        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, 'user', ?, datetime('now'))"
        )
        .bind(&user_msg_id)
        .bind(cid)
        .bind(&req.query)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 寫入 messages 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, sources, profile_used, confidence_score, response_time_ms, created_at) VALUES (?, ?, 'assistant', ?, ?, ?, ?, ?, datetime('now'))"
        )
        .bind(&assistant_msg_id)
        .bind(cid)
        .bind(&answer)
        .bind(&sources_str)
        .bind(&profile)
        .bind(total_score)
        .bind(100i64)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 寫入 messages 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    Ok(Json(json!({
        "queryId": query_id,
        "answer": answer,
        "sources": sources_mapped,
        "confidence": confidence_json,
        "route": "rag",
        "profile": profile,
    })))
}
