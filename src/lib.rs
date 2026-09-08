//! Library half of Luxury TUI, split out from the binary so integration
//! tests (see `tests/cluster_integration.rs`) can exercise the real
//! Kubernetes-facing code against a `kind`/`kwok` cluster in CI.

pub mod app;
pub mod config;
pub mod event;
pub mod k8s;
pub mod logo;
pub mod tasks;
pub mod theme;
pub mod ui;
