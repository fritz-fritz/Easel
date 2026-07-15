# ADR 0001: Rust core with Qt Quick and CXX-Qt

- Status: accepted
- Date: 2026-07-14

## Context

Wallspan needs a modern cross-platform interface, strong native desktop integration, and a safe
concurrent image-processing core. Direct Qt Widgets bindings expose a pointer-heavy API to Rust,
while QML is designed to consume QObject presentation models.

## Decision

Use Rust for domain, application, rendering, provider, scheduling, and platform logic. Use Qt 6
Quick Controls for the interface and CXX-Qt for a narrow QObject/model bridge.

QML contains presentation behavior only. Business state and mutations remain in Rust.

## Consequences

- Qt and a C++ compiler are build dependencies for the desktop application.
- The non-Qt crates remain independently testable and reusable.
- Platform-native Qt styling is available without adopting a separate Rust GUI toolkit.
- CXX-Qt generated code is a deliberate boundary that requires dedicated CI coverage.
