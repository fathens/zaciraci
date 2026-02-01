//! CLI tokens テストモジュール
//!
//! テストは以下のサブモジュールに整理されています：
//! - `unit`: 基本的な構造体や関数の単体テスト
//! - `integration`: コマンド間の連携や統合的な機能のテスト
//! - `api`: 外部API（Backend、Chronos）との連携テスト
//! - `predict_args`: predict コマンドの詳細なパラメータテスト
//! - `environment`: 環境変数とワークスペース設定のテスト

#[cfg(test)]
mod unit;

#[cfg(test)]
mod integration;

#[cfg(test)]
mod api;

#[cfg(test)]
mod predict_args;

#[cfg(test)]
mod environment;
