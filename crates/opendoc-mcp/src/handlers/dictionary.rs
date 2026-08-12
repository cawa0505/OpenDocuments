use std::sync::Arc;
// removed unused std::collections::HashMap
use crate::utils::resolve_workspace_id;
use crate::McpState;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    pub id: String,
    pub workspace_id: String,
    pub key: String,
    pub value: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct AddEntryReq {
    pub key: String,
    pub value: String,
}

#[derive(Deserialize)]
pub struct ImportSeedReq {
    pub language: String,
}

pub async fn get_dictionary_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query("SELECT id, workspace_id, key, value, datetime(created_at, 'localtime') FROM dictionary WHERE workspace_id = ?")
        .bind(&workspace_id)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢 dictionary 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut entries = Vec::new();
    for r in rows {
        entries.push(DictionaryEntry {
            id: sqlx::Row::get(&r, 0),
            workspace_id: sqlx::Row::get(&r, 1),
            key: sqlx::Row::get(&r, 2),
            value: sqlx::Row::get(&r, 3),
            created_at: sqlx::Row::get::<Option<String>, _>(&r, 4).unwrap_or_default(),
        });
    }

    Ok(Json(json!({ "entries": entries })))
}

pub async fn add_dictionary_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<AddEntryReq>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let trimmed_key = req.key.trim().to_string();
    let trimmed_value = req.value.trim().to_string();

    if trimmed_key.is_empty() || trimmed_value.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 檢查 key 是否在目前工作空間中已存在
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM dictionary WHERE workspace_id = ? AND key = ?")
            .bind(&workspace_id)
            .bind(&trimmed_key)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|e| {
                eprintln!("💥 檢查 dictionary 鍵失敗: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let entry = if let Some((id,)) = existing {
        sqlx::query("UPDATE dictionary SET value = ?, created_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(&trimmed_value)
            .bind(&id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                eprintln!("💥 更新 dictionary 失敗: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        DictionaryEntry {
            id,
            workspace_id,
            key: trimmed_key,
            value: trimmed_value,
            created_at: now,
        }
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO dictionary (id, workspace_id, key, value, created_at) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)")
            .bind(&id)
            .bind(&workspace_id)
            .bind(&trimmed_key)
            .bind(&trimmed_value)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                eprintln!("💥 插入 dictionary 失敗: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        DictionaryEntry {
            id,
            workspace_id,
            key: trimmed_key,
            value: trimmed_value,
            created_at: now,
        }
    };

    Ok(Json(json!(entry)))
}

pub async fn delete_dictionary_handler(
    State(state): State<Arc<McpState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    sqlx::query("DELETE FROM dictionary WHERE id = ?")
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 dictionary 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "deleted": true })))
}

pub async fn import_seed_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ImportSeedReq>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let glossary: std::collections::HashMap<&str, &str> = if req.language == "zh-TW" {
        vec![
            ("認證", "authentication"),
            ("設定", "configuration"),
            ("部署", "deployment"),
            ("安裝", "installation"),
            ("資料庫", "database"),
            ("伺服器", "server"),
            ("客戶端", "client"),
            ("使用者", "user"),
            ("管理員", "admin"),
            ("安全性", "security"),
            ("權限", "permission"),
            ("登入", "login"),
            ("密碼", "password"),
            ("搜尋", "search"),
            ("文檔", "document"),
            ("檔案", "file"),
            ("上傳", "upload"),
            ("下載", "download"),
            ("錯誤", "error"),
            ("除錯", "debugging"),
            ("修復", "fix"),
            ("測試", "test"),
            ("建置", "build"),
            ("執行", "run"),
            ("函式", "function"),
            ("變數", "variable"),
            ("型態", "type"),
            ("模組", "module"),
            ("套件", "package"),
            ("程式庫", "library"),
            ("框架", "framework"),
            ("元件", "component"),
            ("介面", "interface"),
            ("環境變數", "environment variable"),
            ("快取", "cache"),
            ("佇列", "queue"),
            ("架構", "architecture"),
            ("微服務", "microservice"),
            ("設計", "design"),
            ("模式", "pattern"),
            ("依賴", "dependency"),
            ("擴充性", "scalability"),
            ("中介軟體", "middleware"),
            ("端點", "endpoint"),
            ("路由", "routing"),
            ("閘道", "gateway"),
            ("代理", "proxy"),
            ("負載平衡", "load balancer"),
            ("服務網格", "service mesh"),
            ("單體", "monolithic"),
            ("後端", "backend"),
            ("前端", "frontend"),
            ("容器", "container"),
            ("管線", "pipeline"),
            ("監控", "monitoring"),
            ("基礎設施", "infrastructure"),
            ("雲端", "cloud"),
            ("健康檢查", "health check"),
            ("命名空間", "namespace"),
            ("節點", "node"),
            ("服務", "service"),
            ("資料卷", "volume"),
            ("機密", "secret"),
            ("備份", "backup"),
            ("還原", "restore"),
            ("查詢", "query"),
            ("索引", "index"),
            ("交易", "transaction"),
            ("向量", "vector"),
            ("嵌入", "embedding"),
            ("相似度", "similarity"),
            ("連線池", "connection pool"),
            ("機器學習", "machine learning"),
            ("推論", "inference"),
            ("微調", "fine-tuning"),
            ("模型", "model"),
            ("詞記", "token"),
            ("檢索增強生成", "retrieval augmented generation"),
            ("切片", "chunking"),
            ("重排", "reranking"),
        ]
        .into_iter()
        .collect()
    } else if req.language == "ko-KR" {
        vec![
            ("인증", "authentication"),
            ("설정", "configuration"),
            ("배포", "deployment"),
            ("설치", "installation"),
            ("데이터베이스", "database"),
            ("서버", "server"),
            ("클라이언트", "client"),
            ("사용자", "user"),
            ("관리자", "admin"),
            ("보안", "security"),
            ("권한", "permission"),
            ("로그인", "login"),
            ("비밀번호", "password"),
            ("검색", "search"),
            ("문서", "document"),
            ("파일", "file"),
            ("업로드", "upload"),
            ("다운로드", "download"),
            ("오류", "error"),
            ("디버깅", "debugging"),
            ("수정", "fix"),
            ("테스트", "test"),
            ("빌드", "build"),
            ("실행", "run"),
            ("함수", "function"),
            ("변수", "variable"),
            ("타입", "type"),
            ("모듈", "module"),
            ("패키지", "package"),
            ("라이브러리", "library"),
            ("프레임워크", "framework"),
            ("컴포넌트", "component"),
            ("인터페이스", "interface"),
            ("환경 변수", "environment variable"),
            ("캐시", "cache"),
            ("큐", "queue"),
            ("아키텍처", "architecture"),
            ("마이크로서비스", "microservice"),
            ("디자인", "design"),
            ("패턴", "pattern"),
            ("의존성", "dependency"),
            ("확장성", "scalability"),
            ("미들웨어", "middleware"),
            ("엔드포인트", "endpoint"),
            ("라우팅", "routing"),
            ("게이트웨이", "gateway"),
            ("프록시", "proxy"),
            ("로드 밸런서", "load balancer"),
            ("서비스 메시", "service mesh"),
            ("모놀리식", "monolithic"),
            ("백엔드", "backend"),
            ("프론트엔드", "frontend"),
            ("컨테이너", "container"),
            ("파이프라인", "pipeline"),
            ("모니터링", "monitoring"),
            ("인프라", "infrastructure"),
            ("클라우드", "cloud"),
            ("헬스 체크", "health check"),
            ("네임스페이스", "namespace"),
            ("노드", "node"),
            ("서비스", "service"),
            ("볼륨", "volume"),
            ("비밀", "secret"),
            ("백업", "backup"),
            ("복구", "restore"),
            ("질의", "query"),
            ("인덱스", "index"),
            ("트랜잭션", "transaction"),
            ("벡터", "vector"),
            ("임베딩", "embedding"),
            ("유사도", "similarity"),
            ("커넥션 풀", "connection pool"),
            ("머신 러닝", "machine learning"),
            ("추론", "inference"),
            ("파인 튜닝", "fine-tuning"),
            ("모델", "model"),
            ("토큰", "token"),
            ("검색 증강 생성", "retrieval augmented generation"),
            ("청킹", "chunking"),
            ("리랭킹", "reranking"),
        ]
        .into_iter()
        .collect()
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };

    // 使用 Transaction 進行原子批量 UPSERT 寫入
    let mut tx = state.db_pool.begin().await.map_err(|e| {
        eprintln!("💥 無法開啟 Transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    for (k, v) in glossary {
        let trimmed_key = k.trim().to_string();
        let trimmed_value = v.trim().to_string();

        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM dictionary WHERE workspace_id = ? AND key = ?")
                .bind(&workspace_id)
                .bind(&trimmed_key)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| {
                    eprintln!("💥 交易中查詢 dictionary 失敗: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

        if let Some((id,)) = existing {
            sqlx::query(
                "UPDATE dictionary SET value = ?, created_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(&trimmed_value)
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                eprintln!("💥 交易中更新 dictionary 失敗: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO dictionary (id, workspace_id, key, value, created_at) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)")
                .bind(&id)
                .bind(&workspace_id)
                .bind(&trimmed_key)
                .bind(&trimmed_value)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    eprintln!("💥 交易中插入 dictionary 失敗: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        }
    }

    tx.commit().await.map_err(|e| {
        eprintln!("💥 提交 Transaction 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({ "imported": true })))
}
