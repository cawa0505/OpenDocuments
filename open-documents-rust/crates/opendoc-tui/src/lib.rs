#![deny(clippy::unwrap_used)]
#![deny(clippy::clone_on_ref_ptr)]
#![warn(clippy::pedantic)]

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Row, Table, Cell},
    Frame,
};
use crossterm::event::KeyCode;

/// 終端機 TUI 接收的內部/外部事件型別
#[derive(Debug, Clone)]
pub enum TuiEvent {
    Input(KeyCode),
    Tick,
    FetchResults(Vec<TuiSearchResult>), // 背景混合檢索完畢後丟回來的數據
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TuiSearchResult {
    pub file_name: String,
    pub score: f32,
    pub snippet: String,
}

pub struct TuiAppState {
    pub search_query: String,
    pub results: Vec<TuiSearchResult>,
    pub active_workspace: String,
}

impl TuiAppState {
    #[must_use]
    pub fn new(active_workspace: String) -> Self {
        Self {
            search_query: String::new(),
            results: Vec::new(),
            active_workspace,
        }
    }
}

/// 核心渲染函數：每次 Tick 或事件觸發時被立即調用
pub fn render_ui(f: &mut Frame<'_>, state: &TuiAppState) {
    let size = f.size();

    // 1. 極端尺寸邊界防禦 (防止極小尺寸下計算 Layout 導致 Panic)
    if size.width < 50 || size.height < 10 {
        let warning = Paragraph::new(" ⚠️ 視窗太小，請放大終端機以顯示 RAG 檢索面板... ")
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).title(" 🚨 視窗尺寸過窄 "));
        f.render_widget(warning, size);
        return;
    }

    // 2. 將終端機畫面垂直切分成兩塊：上方搜尋框 (佔3行)，下方結果面板 (佔滿剩餘空間)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // 搜尋框固定高度
            Constraint::Min(0),    // 結果區域自適應
        ])
        .split(size);

    // 3. 渲染上方搜尋框 (帶有當前工作區提示)
    let search_title = format!(" 🔍 OpenDocuments 混合檢索 [{}] (按 Esc 退出) ", state.active_workspace);
    let search_block = Paragraph::new(state.search_query.as_str())
        .block(Block::default()
            .borders(Borders::ALL)
            .title(search_title)
            .border_style(Style::default().fg(Color::Cyan))
        );
    f.render_widget(search_block, chunks[0]);

    // 4. 動態斷點響應式設計：根據視窗寬度決定是否隱藏 Score 欄位 (TUI 版 Media Queries)
    if size.width < 85 {
        // 中等偏窄視窗：丟棄 Score 欄位，騰出空間給檔案名稱與 Snippet 
        let rows: Vec<Row> = state.results.iter().map(|r| {
            Row::new(vec![
                Cell::from(r.file_name.as_str()).style(Style::default().fg(Color::White)),
                Cell::from(r.snippet.as_str()).style(Style::default().fg(Color::Gray)),
            ])
        }).collect();

        let table = Table::new(rows, [
            Constraint::Percentage(30), // 檔案名稱放寬至 30%
            Constraint::Percentage(70), // Snippet 佔 70%
        ])
        .header(Row::new(vec!["檔案名稱", "內容摘要 (FTS5 / Vector)"])
            .style(Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD))
        )
        .block(Block::default().borders(Borders::ALL).title(" 📄 檢索結果 (分數已自動隱藏) "));

        f.render_widget(table, chunks[1]);
    } else {
        // 寬視窗：展示完整 3 欄
        let rows: Vec<Row> = state.results.iter().map(|r| {
            // 針對 Score Filter 的分數高亮：大於 0.75 顯示綠色，其餘黃色
            let score_color = if r.score >= 0.75 { Color::Green } else { Color::Yellow };
            
            Row::new(vec![
                Cell::from(r.file_name.as_str()).style(Style::default().fg(Color::White)),
                Cell::from(format!("{:.2}", r.score)).style(Style::default().fg(score_color).add_modifier(Modifier::BOLD)),
                Cell::from(r.snippet.as_str()).style(Style::default().fg(Color::Gray)),
            ])
        }).collect();

        let table = Table::new(rows, [
            Constraint::Percentage(20), // 檔案名稱欄寬
            Constraint::Percentage(10), // Score 欄寬
            Constraint::Percentage(70), // 內文片段欄寬
        ])
        .header(Row::new(vec!["檔案名稱", "Score", "內容摘要 (FTS5 / Vector)"])
            .style(Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD))
        )
        .block(Block::default().borders(Borders::ALL).title(" 📄 檢索結果 (Score Filter 作用中) "));

        f.render_widget(table, chunks[1]);
    }
}
