//! Simulateコマンドのテストモジュール
//!
//! テストは以下のサブモジュールに整理されています：
//! - `unit`: 基本的な単体テスト（SimulateArgs、リバランス間隔、取引ロジック等）
//! - `integration`: 統合テスト（パフォーマンス指標、トレード実行統合等）

#[cfg(test)]
mod unit;

#[cfg(test)]
mod integration;
