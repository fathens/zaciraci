// Zaciraci Library
// Protobuf ベースのサービス実装

// Proto定義モジュール（自動生成コードをラップ）
// 新しいスタイル：単一のgeneratedモジュールに統合
// pub mod proto;
pub mod generated;

// サービスの実装
pub mod services;

// RPC実装
pub mod rpc;

// webサーバー（gRPC実装）
pub mod web;
