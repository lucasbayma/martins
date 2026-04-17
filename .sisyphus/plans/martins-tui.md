# Martins — TUI para gerenciar times de agentes de IA

## TL;DR

> **Quick Summary**: Aplicação TUI em Rust, inspirada em Conductor.build, que gerencia agentes de IA (opencode/claude/codex) em workspaces isolados via git worktrees. Layout de 3 painéis: repos+workspaces à esquerda, terminal PTY embutido no centro, arquivos modificados à direita. Nomes de workspace auto-gerados a partir de uma lista curada de 50 artistas MPB.
>
> **Deliverables**:
> - Binário `martins` para macOS + Linux
> - 3-pane TUI com modo Normal/Terminal explícito (estilo vim/turborepo)
> - CRUD de git worktrees (criar, arquivar, desarquivar, deletar) com nomes MPB
> - Terminal PTY embutido (1 workspace ativo visível, múltiplas tabs, outros em background)
> - Sidebar de arquivos modificados (git diff vs base branch, auto-refresh via notify)
> - Preview com `bat` em overlay + abrir em `$EDITOR`
> - Persistência em `.martins/state.json` (atomic writes, schema versionado, gitignore automático)
> - Pre-flight check + auto-install de binários ausentes com confirmação
> - Fuzzy search de workspaces e arquivos modificados
> - **Distribuição**: GitHub Releases com universal binary macOS (arm64+x86_64) + binary Linux x86_64, trigger via tag `v*.*.*`, changelog automatizado via git-cliff, smoke test pré-publish
> - **Homebrew tap** próprio (`brew tap {owner}/martins && brew install martins`) auto-atualizado via CI após release publicado
>
> **Estimated Effort**: XL (~26 tasks, 3-4 semanas full-time)
> **Parallel Execution**: YES — 4 waves + final verification
> **Critical Path**: T1 → T3 → T8 (input modes) → T11 (PTY lifecycle) → T17 (integration) → T22 (shutdown) → T25 (release CI) → F1-F4 → user okay

---

## Context

### Original Request
Usuário quer criar uma aplicação com "feellike de terminal" baseada em https://www.conductor.build para gerenciar times de agentes via terminal, escrita em Rust. 3 painéis (sidebar repos/workspaces, terminal central, arquivos modificados). Workspaces são git worktrees com nomes MPB auto-gerados.

### Interview Summary

**Key Discussions**:
- Agentes suportados: opencode (default, com openagent), claude code, codex — usuário escolhe na criação
- Concorrência: 1 workspace visível, outros em background com PTY vivo
- "Arquivos modificados" = `git diff` vs base branch do worktree (capturada na criação)
- Editor externo: `$EDITOR`/`$VISUAL` (padrão Unix)
- Persistência: `.martins/state.json` **dentro do próprio repo** (gitignorado)
- Archive preserva worktree; delete remove worktree (+ opcional branch)
- MVP inclui: layout+nav, PTY funcional, sidebar arquivos, preview bat, $EDITOR open, persistência JSON, múltiplas tabs, fuzzy search
- TDD com cargo test + ratatui TestBackend + insta snapshots

**Research Findings**:
- **Stack canônica**: ratatui 0.30 + crossterm 0.29 + tui-term 0.3.4 + portable-pty 0.9 + vt100 + tokio 1.36 + git2 0.17 + notify 8.2
- **Conductor mental model**: "sou gerente de time de agentes", não "estou codando" — worktrees são invisíveis ao usuário
- **Projetos de referência de produção**: turborepo-ui, nx tui, zellij, atuin, ratatui-toolkit, tui-term exemplos (nested_shell_async)
- **Lista MPB curada**: 50 artistas ASCII-normalizados (caetano, gil, elis, chico, milton, djavan, marisa, etc.)

### Metis Review

**Identified Gaps** (all resolved via user answers):

1. **Input mode switching** → Modo explícito Normal/Terminal (estilo vim/turborepo): `i` entra em Terminal, `Ctrl+B` ou `Esc Esc` volta pro Normal. Status bar e border color indicam modo.
2. **.martins/ gitignore** → Martins adiciona `.martins/` ao `.gitignore` automaticamente na primeira execução.
3. **Binários ausentes** → Pre-flight check no startup + modal oferecendo auto-install (detecta brew/apt/cargo).
4. **Tamanho mínimo** → 100×30 ótimo, colapsa sidebars em larguras menores, bloqueia abaixo de 80×24.
5. **Un-archive** → Tecla `u` desarquiva; agente não reinicia automaticamente.

**Guardrails aplicados**:
- git2 SEMPRE via `spawn_blocking`, NUNCA no main loop
- Repository aberto fresh por operação (não compartilhar via Arc<Mutex>)
- vt100::Parser por tab, `Arc<RwLock>`, `try_read()` no render
- Atomic write (tmp + rename) para state.json
- Schema versionado desde v1
- EOF detection → status Exited(code)
- Scrollback 1000 linhas por parser
- Graceful shutdown: SIGHUP → 3s timer → SIGKILL → save state → restore terminal

---

## Work Objectives

### Core Objective
Entregar um binário Rust `martins` que roda em macOS e Linux, apresentando uma TUI de 3 painéis que gerencia workspaces (git worktrees) com agentes de IA rodando em terminais PTY embutidos, com persistência local por repo e experiência de navegação modal (Normal/Terminal) previsível.

### Concrete Deliverables
- `Cargo.toml` com dependências pinadas (ratatui 0.30, tui-term 0.3.4, portable-pty 0.9, tokio 1.36, git2 0.17, notify 8.2, serde, serde_json, notify-debouncer-mini, directories, crossterm 0.29)
- Binário `martins` (installable via `cargo install --path .`)
- Estrutura de módulos: `src/main.rs`, `src/app.rs`, `src/config.rs`, `src/state.rs`, `src/mpb.rs`, `src/git/{repo,worktree,diff}.rs`, `src/pty/{session,manager}.rs`, `src/watcher.rs`, `src/ui/{sidebar_left,sidebar_right,terminal,modal,picker,preview}.rs`, `src/agents.rs`, `src/editor.rs`, `src/keys.rs`, `src/tools.rs` (pre-flight/install)
- Lista estática de 50 artistas MPB em `src/mpb.rs`
- Testes unitários em `cargo test` (TestBackend + insta snapshots)
- README com instruções de instalação e uso

### Definition of Done
- [x] `cargo build --release` produz binário funcional no macOS e Linux
- [x] `cargo test` passa com 100% dos testes (sem meta numérica de coverage no MVP — coverage measurement é v2)
- [x] `cargo clippy --all-targets -- -D warnings` passa sem erros
- [x] `cargo fmt --check` passa
- [x] Smoke test E2E via tmux: `cd {tempdir-com-git-init-e-1-commit}` → `martins` (detecta o repo via `Repository::discover`) → criar workspace → spawn bash → modificar arquivo → ver no sidebar direito → arquivar → sair → re-abrir dentro do mesmo repo → workspace arquivado persistiu
- [ ] Release workflow dispara em tag `v0.1.0`, gera universal binary macOS + Linux x86_64, smoke test passa, release draft criada
- [ ] Homebrew formula sintaxe válida; `brew tap` + `brew install` instala o binário corretamente (teste manual após primeiro release)
- [ ] Snapshot tests determinísticos: arquivos `*.snap` committados no repo; `INSTA_UPDATE=no cargo test` passa sem diffs
- [ ] Graceful shutdown: 3 PTYs ativas → Ctrl+Q → processos mortos em <5s → state.json atualizado → terminal restaurado

### Must Have
- **Escopo single-repo no MVP**: martins opera sobre UM repo por vez — o repo determinado pelo `cwd` quando o binário é executado (via `Repository::discover`). Sem UI para "adicionar/trocar repos". Sem persistência de múltiplos repos.
- 3-pane layout com navegação Normal/Terminal modal
- CRUD completo de workspaces (criar/arquivar/desarquivar/deletar)
- PTY embutido rodando opencode (default), claude, ou codex
- Nomes MPB auto-gerados com collision handling
- Sidebar direita com git diff vs base branch, auto-refresh
- Preview `bat` em overlay + `$EDITOR` integration
- Persistência `.martins/state.json` gitignorada, atomic write, schema v1
- Pre-flight check de binários + auto-install com confirmação
- Múltiplas tabs de terminal por workspace (max 5)
- Fuzzy search (workspaces + modified files)
- Graceful shutdown com save de estado
- Resize propagation PTY ↔ UI
- **Release automation**: GitHub Actions workflow triggered by `v*.*.*` tags — builds universal macOS binary (arm64+x86_64 via lipo) + Linux x86_64 binary, runs smoke tests, publishes draft GitHub Release with changelog (git-cliff)
- **Homebrew distribution**: formula em `packaging/homebrew/martins.rb` + CI job que atualiza tap repo separado automaticamente após publish. Usuários instalam via `brew tap {owner}/martins && brew install martins`

### Must NOT Have (Guardrails)
- Chat UI conversacional com agente (fora de escopo)
- Diff viewer interativo, merge actions, PR creation (fora de escopo)
- Integração GitHub/Linear (fora de escopo)
- Setup scripts automáticos (v2)
- Terminal splits (só tabs)
- Windows como target prioritário (só macOS+Linux)
- Syntax highlighting custom com syntect em ratatui (usar bat externo)
- Parse/inspeção da saída do agente (agentes são opacos)
- Git writes (commit, push, stage) — apenas leitura
- **Multi-repo no MVP**: sem lista de repos na sidebar, sem modal AddRepo, sem persistência entre repos diferentes. Cada repo tem seu próprio `.martins/state.json` isolado. (Multi-repo pode ser v2.)
- **Packaging fora de escopo (MVP)**: SEM `.dmg`, SEM codesign/Developer ID, SEM notarization (notarytool/stapler), SEM Sparkle auto-update, SEM instalador `.pkg`, SEM AUR, SEM crates.io publish, SEM pacote `.deb`/`.rpm`, SEM Linux ARM64. Distribuição MVP = tarballs em GitHub Releases + Homebrew tap próprio. (Apple Developer account e pacotes Linux nativos podem ser v2.)
- **Auto-update in-app**: martins NÃO verifica updates nem se auto-atualiza. Usuário atualiza manualmente via `brew upgrade martins` ou baixando novo release.
- Tab bar renomeável/arrastável (tabs numeradas 1-5)
- Busca de conteúdo de arquivos no fuzzy (só nomes)
- Custom agent args ou env vars per-workspace no MVP
- Compartilhar `git2::Repository` via `Arc<Mutex>` (sempre abrir fresh)
- Chamar `git2` no main event loop thread (sempre `spawn_blocking`)
- Acceptance criteria com "verify it works" ou "check the output" (sempre binário)

---

## Verification Strategy

> **ZERO HUMAN INTERVENTION** — Toda verificação é agent-executed via `cargo test`, snapshot diffing (insta), e smoke tests em tmux/interactive_bash. Acceptance criteria requerendo "user manually tests" são PROIBIDOS.

### Test Decision
- **Infrastructure exists**: NO (repo empty, README 9 bytes)
- **Automated tests**: YES (TDD)
- **Framework**: `cargo test` (built-in) + `insta` 1.40+ para snapshot tests + `tempfile` 3.10+ para temp repos + `assert_cmd` 2.0+ para CLI smoke
- **TDD flow**: Cada task segue RED (failing test) → GREEN (minimal impl) → REFACTOR

### QA Policy
Toda task inclui **cenários QA agent-executed** com evidências em `.sisyphus/evidence/task-{N}-{slug}.{ext}`.

- **TUI rendering**: `ratatui::backend::TestBackend` + `insta::assert_snapshot!` (comparação determinística)
- **CLI smoke / E2E**: `Bash` com tmux detached sessions (`tmux new-session -d`, `tmux send-keys`, `tmux capture-pane -p`). Todo teste TUI usa este padrão — sem harness custom, apenas tmux + shell commands.
- **Git ops**: `tempfile::TempDir` com git init + commits fixtures
- **Binários externos**: `assert_cmd` para invocação + validação de exit code e stdout
- **PTY ops**: spawn `/bin/echo hello` ou `/bin/sh -c "printf X; sleep 0.1; exit 42"` e verificar EOF + exit code

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately — foundation):
├── T1:  Cargo project skeleton + deps + CI
├── T2:  Module layout + types base (no impl)
├── T3:  MPB artist list + name generator (pure fn)
├── T4:  State schema (serde) + atomic persistence
├── T5:  Config paths + XDG fallback
├── T6:  Pre-flight binary detection + install command mapping
└── T7:  Logging/tracing setup

Wave 2 (After Wave 1 — domain + infra, MAX PARALLEL):
├── T8:  Input modes (Normal/Terminal) + keymap registry
├── T9:  Git repo ops (open, discover, HEAD, base branch capture)
├── T10: Git worktree CRUD (create, list, prune) via git2
├── T11: PTY session lifecycle (spawn, read loop, EOF, resize, kill)
├── T12: File watcher (notify + debouncer, filter .git/target/node_modules)
├── T13: Git diff vs base branch (modified file list + status)
└── T14: .gitignore auto-edit + first-run bootstrap

Wave 3 (After Wave 2 — UI panes + terminal widget):
├── T15: Layout 3-pane responsive (collapse sidebars ≤120/≤100/<80)
├── T16: Sidebar left (repos → workspaces tree, archived section)
├── T17: Sidebar right (modified files list, status icons)
├── T18: Terminal pane (tui-term PseudoTerminal wrapper + tabs)
├── T19: Modal system (new workspace, confirm delete, install bins)
├── T20: Fuzzy picker (workspaces + modified files)
└── T21: Bat preview overlay + $EDITOR spawn

Wave 4 (After Wave 3 — integration + polish + distribution):
├── T22: Main event loop (async multiplex + shutdown sequence)
├── T23: Agent selection + pre-flight at creation time
├── T24: README + install instructions
├── T25: Release CI (universal macOS + Linux binary + GitHub Release + changelog)
└── T26: Homebrew tap formula + CI tap auto-update

Wave FINAL (All 4 in parallel, then user approval):
├── F1: Plan compliance audit (oracle)
├── F2: Code quality review (unspecified-high)
├── F3: Real manual QA E2E via tmux (unspecified-high)
└── F4: Scope fidelity check (deep)
→ Apresenta resultados → espera explicit user okay

Critical Path: T1 → T2 → T8 → T11 → T18 → T22 → T25 → F1-F4 → okay
Parallel Speedup: ~65% faster than sequential
Max Concurrent: 7 (Wave 1 & 2)
```

### Dependency Matrix

- **T1** (cargo skeleton): none → unblocks T2, T3, T4, T5, T6, T7
- **T2** (module layout): T1 → unblocks T8, T9, T10, T11, T12, T13, T14
- **T3** (MPB names): T1 → unblocks T19
- **T4** (state schema): T1, T2 → unblocks T10, T14, T22
- **T5** (config paths): T1 → unblocks T4, T6
- **T6** (pre-flight bins): T1, T5 → unblocks T19, T23
- **T7** (logging): T1 → unblocks all (logging everywhere)
- **T8** (input modes): T2 → unblocks T15, T18, T22
- **T9** (git repo): T2 → unblocks T10, T13
- **T10** (worktree CRUD): T2, T4, T9 → unblocks T16, T19, T22
- **T11** (PTY session): T2 → unblocks T18, T22
- **T12** (file watcher): T2 → unblocks T13, T17, T22
- **T13** (git diff): T9, T12 → unblocks T17
- **T14** (gitignore bootstrap): T4, T9 → unblocks T22
- **T15** (layout responsive): T8 → unblocks T16, T17, T18
- **T16** (sidebar left): T10, T15 → unblocks T22
- **T17** (sidebar right): T13, T15 → unblocks T21, T22
- **T18** (terminal pane): T8, T11, T15 → unblocks T22
- **T19** (modals): T3, T6, T10 → unblocks T22
- **T20** (fuzzy picker): T10, T13 → unblocks T22
- **T21** (preview+editor): T17 → unblocks T22
- **T22** (main loop): T4, T10, T11, T14, T16, T17, T18, T19, T20, T21 → unblocks T24, T25, F1-F4
- **T23** (agent selection+preflight): T6, T19 → unblocks T24, T25, F1-F4
- **T24** (README): T22, T23 → unblocks F1-F4 (also referenced by T26)
- **T25** (release CI): T22, T23 → unblocks T26, F1-F4
- **T26** (homebrew tap): T25 → unblocks F1, F3

### Agent Dispatch Summary

- **Wave 1 (7 tasks)**: T1→`quick`, T2→`quick`, T3→`quick`, T4→`deep`, T5→`quick`, T6→`unspecified-high`, T7→`quick`
- **Wave 2 (7 tasks)**: T8→`deep`, T9→`deep`, T10→`deep`, T11→`ultrabrain`, T12→`unspecified-high`, T13→`deep`, T14→`quick`
- **Wave 3 (7 tasks)**: T15→`visual-engineering`, T16→`visual-engineering`, T17→`visual-engineering`, T18→`ultrabrain`, T19→`visual-engineering`, T20→`visual-engineering`, T21→`unspecified-high`
- **Wave 4 (5 tasks)**: T22→`ultrabrain`, T23→`deep`, T24→`writing`, T25→`deep`, T26→`unspecified-high`
- **Final (4 tasks)**: F1→`oracle`, F2→`unspecified-high`, F3→`unspecified-high`, F4→`deep`

---

## TODOs

> Implementação + Test = UMA task. QA Scenarios são obrigatórios.
> Skills sempre REQUIRED: pass `[]` se nenhuma. Para Rust, skills úteis: `git-master` (git ops).

### Wave 1 — Foundation (T1-T7, paralelizáveis)

- [x] 1. **Cargo project skeleton + pinned dependencies + CI**

  **What to do**:
  - Criar `Cargo.toml` com `[package]` name="martins", edition="2024", rust-version="1.85"
  - Adicionar deps pinadas (lista completa — cobre todas as tasks):
    - TUI: `ratatui = "0.30"`, `crossterm = { version = "0.29", features = ["event-stream"] }`, `tui-term = { version = "0.3.4", features = ["unstable"] }`, `vt100 = "0.16"`
    - PTY: `portable-pty = "0.9"`
    - Async: `tokio = { version = "1.36", features = ["full"] }`, `futures = "0.3"`
    - Git: `git2 = "0.17"`
    - FS: `notify = "8.2"`, `notify-debouncer-mini = "0.4"`
    - Serde: `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `chrono = { version = "0.4", features = ["serde"] }`
    - Paths: `directories = "5"`
    - Utility: `which = "6"`, `sha2 = "0.10"`, `fastrand = "2"`, `unicode-normalization = "0.1"`, `nucleo-matcher = "0.3"`
    - Logging: `tracing = "0.1"`, `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`, `tracing-appender = "0.2"`
    - Errors: `anyhow = "1"`, `thiserror = "1"`
  - Dev-deps: `insta = "1.40"`, `tempfile = "3.10"`, `assert_cmd = "2.0"`, `predicates = "3"`
  - Criar `src/main.rs` hello-world que imprime "martins 0.1.0"
  - Criar `.gitignore` com `target/`, `.idea/`, `*.swp`
  - Criar diretório `.sisyphus/evidence/` com `.gitkeep` para que QA scenarios possam escrever logs ali (sem isso, todos os `tee .sisyphus/evidence/*` falham com "No such file or directory")
  - Criar `rustfmt.toml` com `edition = "2024"`, `max_width = 100`, `imports_granularity = "Crate"`
  - Criar `clippy.toml` com `avoid-breaking-exported-api = false`
  - Criar `.github/workflows/ci.yml` rodando fmt + clippy + test em macos-latest + ubuntu-latest

  **Must NOT do**:
  - NÃO adicionar deps além da lista acima (zero bloat)
  - NÃO adicionar features opcionais/flags
  - NÃO criar módulos ainda (será T2)

  **Recommended Agent Profile**:
  - **Category**: `quick` — Setup mecânico, sem design decisions
  - **Skills**: `[]` (nenhuma skill específica necessária)

  **Parallelization**:
  - **Can Run In Parallel**: NO (prerequisite de tudo)
  - **Parallel Group**: Wave 1 (primeiro, sequencial)
  - **Blocks**: T2, T3, T4, T5, T6, T7
  - **Blocked By**: None

  **References**:
  - Pattern: https://github.com/ratatui-org/ratatui/blob/main/Cargo.toml — estrutura de deps
  - Pattern: https://github.com/a-kenji/tui-term/blob/main/Cargo.toml — features flag "unstable" para portable-pty
  - Official docs: https://doc.rust-lang.org/cargo/reference/manifest.html — Cargo.toml spec
  - Pattern: https://github.com/vercel/turborepo/blob/main/.github/workflows/rust.yml — matrix CI macOS+Linux

  **WHY Each Reference Matters**:
  - ratatui Cargo.toml: copiar exatamente como `ratatui` declara suas deps (features right)
  - tui-term: flag `unstable` é OBRIGATÓRIA para ativar portable-pty support
  - turborepo workflow: matrix fmt+clippy+test cobre nossos dois targets

  **Acceptance Criteria**:

  **If TDD**:
  - [ ] `cargo build` sucesso
  - [ ] `cargo fmt --check` passa
  - [ ] `cargo clippy -- -D warnings` passa (sem código ainda, trivial)
  - [ ] CI workflow sintaxe válida: `gh workflow view ci.yml` ou `actionlint .github/workflows/ci.yml`
  - [ ] Diretório `.sisyphus/evidence/.gitkeep` existe e está committed (pré-requisito para QA scenarios subsequentes)

  **QA Scenarios**:

  ```
  Scenario: Binary compiles and prints version
    Tool: Bash
    Preconditions: Fresh clone, rust 1.85+ installed
    Steps:
      1. cargo build --release
      2. ./target/release/martins
    Expected Result: stdout contains "martins 0.1.0", exit code 0
    Failure Indicators: non-zero exit, panic, missing binary
    Evidence: .sisyphus/evidence/task-1-compile.log

  Scenario: Dependency lock file is reproducible
    Tool: Bash
    Preconditions: Cargo.toml committed
    Steps:
      1. cargo update --dry-run 2>&1 | tee /tmp/update.log
      2. grep -E "Updating|Adding" /tmp/update.log | wc -l
    Expected Result: 0 pending updates (lock file is fresh)
    Evidence: .sisyphus/evidence/task-1-lock.log

  Scenario: CI workflow validates
    Tool: Bash
    Preconditions: actionlint installed (brew install actionlint)
    Steps:
      1. actionlint .github/workflows/ci.yml
    Expected Result: exit code 0, no output
    Evidence: .sisyphus/evidence/task-1-ci-lint.log
  ```

  **Commit**: YES — `chore(init): cargo project skeleton with pinned dependencies`

- [x] 2. **Module layout + base types (no impl)**

  **What to do**:
  - Criar arquivos vazios com apenas doc comment + `pub` statements stub:
    - `src/app.rs`, `src/config.rs`, `src/state.rs`, `src/mpb.rs`, `src/tools.rs`, `src/editor.rs`, `src/keys.rs`, `src/agents.rs`, `src/watcher.rs`
    - `src/git/mod.rs`, `src/git/repo.rs`, `src/git/worktree.rs`, `src/git/diff.rs`
    - `src/pty/mod.rs`, `src/pty/session.rs`, `src/pty/manager.rs`
    - `src/ui/mod.rs`, `src/ui/sidebar_left.rs`, `src/ui/sidebar_right.rs`, `src/ui/terminal.rs`, `src/ui/modal.rs`, `src/ui/picker.rs`, `src/ui/preview.rs`, `src/ui/layout.rs`
  - Em `src/main.rs`: declarar todos os módulos (`mod app; mod config; ...`)
  - Definir tipos base em `src/state.rs`:
    ```rust
    pub enum WorkspaceStatus { Active, Inactive, Archived, Exited(i32) }
    pub enum Agent { Opencode, Claude, Codex }
    pub struct Workspace { name, worktree_path, base_branch, agent, status, created_at, tabs }
    pub struct TabSpec { id: u32, command: String }
    pub struct AppState { version: u32, workspaces: Vec<Workspace> }
    ```
  - Definir `InputMode` em `src/keys.rs`: `enum InputMode { Normal, Terminal }`
  - Definir `Error` enum em novo `src/error.rs` com variantes cobrindo todos os erros esperados (IO, Git, Pty, State, Config)

  **Must NOT do**:
  - NÃO implementar lógica em nenhum módulo (apenas definições de tipo)
  - NÃO adicionar derives além de `Debug, Clone, Serialize, Deserialize` nos tipos de estado
  - NÃO criar Traits prematuros (esperar necessidade real)

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: NO (bloqueia Wave 2)
  - **Parallel Group**: Wave 1 depois de T1
  - **Blocks**: T8, T9, T10, T11, T12, T13, T14, T22
  - **Blocked By**: T1

  **References**:
  - Pattern: https://github.com/vercel/turborepo/tree/main/crates/turborepo-ui/src/tui — layout modular de TUI Rust
  - Pattern: https://github.com/atuinsh/atuin/tree/main/atuin-client/src — separação de domínio vs infra
  - Rust book: https://doc.rust-lang.org/book/ch07-00-managing-growing-projects-with-packages-crates-and-modules.html

  **WHY Each Reference Matters**:
  - turborepo-ui: exemplo real de TUI em Rust com separação sidebar/pane/event loop
  - atuin: bom modelo de arquitetura com serde state + domain types

  **Acceptance Criteria**:
  - [ ] `cargo build` sucesso (todos os módulos declarados, sem código ainda)
  - [ ] `cargo clippy -- -D warnings` passa
  - [ ] Tipos base serializam/deserializam via serde_json (teste unitário)

  **QA Scenarios**:

  ```
  Scenario: Module graph compiles
    Tool: Bash
    Preconditions: T1 complete
    Steps:
      1. cargo build 2>&1 | tee /tmp/build.log
      2. grep -c "unused" /tmp/build.log || true
    Expected Result: exit 0, build success. Unused warnings ≤ 5 (stubs expected).
    Evidence: .sisyphus/evidence/task-2-build.log

  Scenario: State serde round-trip
    Tool: Bash
    Preconditions: T1 + T2 complete
    Steps:
      1. cargo test state::tests::roundtrip -- --nocapture
    Expected Result: test passes. Output shows serialized JSON and deserialized matches.
    Evidence: .sisyphus/evidence/task-2-serde.log
  ```

  **Commit**: YES — `feat(core): define module layout and base types`

- [x] 3. **MPB artist list + name generator (pure function, deterministic)**

  **What to do**:
  - Em `src/mpb.rs`: `const ARTISTS: &[&str; 50]` com os 50 nomes EXATOS abaixo (autoridade única — não consultar fontes externas):
    ```rust
    const ARTISTS: &[&str; 50] = &[
        // Bossa nova (5)
        "joao-gilberto", "tom-jobim", "vinicius", "dorival-caymmi", "carlos-lyra",
        // Tropicália (10)
        "caetano", "gil", "gal", "bethania", "tom-ze",
        "chico", "elis", "jorge-ben", "mutantes", "novos-baianos",
        // Clássicos pós-tropicália (10)
        "milton", "djavan", "marisa", "ivan-lins", "joao-bosco",
        "belchior", "alceu", "lo-borges", "edu-lobo", "marcos-valle",
        // Contemporâneos (15)
        "moreno", "bebel", "marisa-monte", "tulipa", "silva",
        "tiago-iorc", "liniker", "luedji", "rubel", "rodrigo-amarante",
        "jeneci", "ceu", "maria-gadu", "mallu", "arnaldo-antunes",
        // Emergentes (10)
        "tim-bernardes", "duda-beat", "flora-matos", "marina-sena", "emicida",
        "bala-desejo", "carne-doce", "kiko-dinucci", "fabiano", "ze-manoel",
    ];
    ```
  - `pub fn generate_name(used: &HashSet<String>) -> String`:
    - Random choice de ARTISTS não em `used`
    - Se todos usados → sufixo `-N` incremental (tentar `caetano-2`, `caetano-3`, ...)
    - Usar `fastrand` (já em deps de T1)
  - `pub fn normalize(name: &str) -> String`: lowercase + strip accents (usando `unicode-normalization` + filter-ascii) + replace spaces→hyphens + strip non-alphanumeric
  - `pub fn validate(name: &str) -> Result<(), NameError>`: reject empty, reject > 40 chars, reject non-[a-z0-9-_], reject starting with `-`

  **Must NOT do**:
  - NÃO hardcodar PRNG seed (exceto em testes)
  - NÃO ler a lista de arquivo externo (embutir estática)
  - NÃO tentar gerar nomes "inteligentemente" (só random+suffix)

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T4, T5, T6, T7
  - **Parallel Group**: Wave 1
  - **Blocks**: T19
  - **Blocked By**: T1

  **References**:
  - Lista canônica: embutida acima neste próprio plano (autoridade única — não buscar externamente)
  - `unicode-normalization` crate: https://docs.rs/unicode-normalization/
  - Example accent stripping: https://stackoverflow.com/questions/69609933/rust-how-to-remove-accents-from-string

  **WHY Each Reference Matters**:
  - Lista embutida: fonte única de verdade dentro do plano (50 nomes, zero dependência externa)
  - unicode-normalization: NFD + filter `!is_ascii_alphanumeric` padrão canônico

  **Acceptance Criteria**:
  - [ ] Teste: `generate_name(&empty)` → retorna um dos 50 nomes
  - [ ] Teste: `generate_name(&{all 50})` → retorna `{artist}-2`
  - [ ] Teste: `generate_name(&{all 50 + all 50 -2})` → retorna `{artist}-3`
  - [ ] Teste: `normalize("João Gilberto")` → `"joao-gilberto"`
  - [ ] Teste: `normalize("Zé Manoel")` → `"ze-manoel"`
  - [ ] Teste: `validate("")` → Err, `validate("caetano")` → Ok, `validate("foo bar")` → Err (space), `validate("-bad")` → Err
  - [ ] ARTISTS.len() == 50, todos passam `validate`

  **QA Scenarios**:

  ```
  Scenario: All 50 MPB names are valid identifiers
    Tool: Bash
    Preconditions: T1+T2+T3 complete
    Steps:
      1. cargo test mpb::tests::all_artists_valid -- --nocapture
    Expected Result: test passes. Validates each of 50 names through validate().
    Evidence: .sisyphus/evidence/task-3-validate.log

  Scenario: Name collision suffix works correctly
    Tool: Bash
    Preconditions: T1+T2+T3 complete
    Steps:
      1. cargo test mpb::tests::collision_handling -- --nocapture
    Expected Result: Seeded PRNG with known seed. After 50 picks returns original names. After 51st pick returns "{name}-2".
    Evidence: .sisyphus/evidence/task-3-collision.log

  Scenario: Accent normalization covers all needed cases
    Tool: Bash
    Preconditions: T1+T2+T3 complete
    Steps:
      1. cargo test mpb::tests::normalize_accents -- --nocapture
    Expected Result: "João" → "joao", "Zé" → "ze", "Bethânia" → "bethania", "Lô Borges" → "lo-borges"
    Evidence: .sisyphus/evidence/task-3-normalize.log
  ```

  **Commit**: YES — `feat(mpb): add brazilian mpb artist names and generator`

- [x] 4. **State schema + atomic persistence**

  **What to do**:
  - Em `src/state.rs`:
    - Struct `AppState { version: u32 = 1, workspaces: Vec<Workspace> }`
    - `impl AppState`:
      - `pub fn load(repo_root: &Path) -> Result<Self>`: lê `.martins/state.json`; se não existe → retorna `Self::default()`; se JSON inválido → tenta `.martins/state.json.bak`; se também inválido → retorna default + loga warning
      - `pub fn save(&self, repo_root: &Path) -> Result<()>` — **sequência exata (ordem importa)**:
        1. Cria `.martins/` se não existe
        2. Se `.martins/state.json` existe E é válido: **copia** `.martins/state.json` → `.martins/state.json.bak` (fs::copy, NÃO rename — preserva o original para o rename atômico seguinte)
        3. Escreve serialização em `.martins/state.json.tmp`
        4. `fs::rename(.martins/state.json.tmp, .martins/state.json)` (atômico no mesmo filesystem)
        5. Se rename falhar → deleta tmp, retorna Err (estado antigo preservado intacto)
      - `pub fn add_workspace(&mut self, ws: Workspace)`, `pub fn archive(&mut self, name: &str)`, `pub fn unarchive(&mut self, name: &str)`, `pub fn remove(&mut self, name: &str)`
      - `pub fn active(&self) -> impl Iterator<Item = &Workspace>`, `pub fn archived(&self) -> impl Iterator<Item = &Workspace>`
      - `pub fn used_names(&self) -> HashSet<String>` — retorna nomes ativos + arquivados (para o generator)
    - Version mismatch: se `state.version != 1`, retornar `Err(StateError::UnsupportedVersion(v))`

  **Must NOT do**:
  - NÃO usar `fs::write` direto (sempre tmp+rename)
  - NÃO fazer merge de states (single source of truth)
  - NÃO implementar migration logic ainda (só version check)

  **Recommended Agent Profile**:
  - **Category**: `deep` — Atomicidade, backup fallback, edge cases de concorrência
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T3, T5, T6, T7
  - **Parallel Group**: Wave 1
  - **Blocks**: T10, T14, T22
  - **Blocked By**: T1, T2

  **References**:
  - Pattern: https://docs.rs/tempfile/latest/tempfile/persist_noclobber.html — atomic rename idiom
  - Pattern: https://github.com/atuinsh/atuin/blob/main/crates/atuin-client/src/settings.rs — serde config loading com fallback
  - Post: https://danluu.com/deconstruct-files/ — por que atomic rename matters
  - `serde_json::to_string_pretty` for human-readable state

  **WHY Each Reference Matters**:
  - tempfile: idiom exato para `persist` atomic (write to tmp, rename)
  - atuin settings.rs: fallback pattern (tenta .json, depois .toml, depois default)
  - danluu article: explica porque `write + sync + rename` é a única forma correta

  **Acceptance Criteria**:
  - [ ] Teste: save → load → equals
  - [ ] Teste: load em dir vazio → retorna default
  - [ ] Teste: save corrompe state.json (escreve bytes aleatórios após save) → load cai em .bak
  - [ ] Teste: save escreve primeiro em state.json.tmp, depois rename (observar via TempDir + listing)
  - [ ] Teste: version = 99 em arquivo → load retorna Err(UnsupportedVersion(99))

  **QA Scenarios**:

  ```
  Scenario: Atomic write survives mid-write crash simulation
    Tool: Bash
    Preconditions: T1+T2+T4 complete
    Steps:
      1. cargo test state::tests::atomic_write -- --nocapture
    Expected Result: test simulates writing only .tmp file (no rename). Next load returns default + original state.json intact.
    Evidence: .sisyphus/evidence/task-4-atomic.log

  Scenario: Backup recovery on corrupted main file
    Tool: Bash
    Preconditions: T1+T2+T4 complete
    Steps:
      1. cargo test state::tests::backup_recovery -- --nocapture
    Expected Result: state.json corrupted + .bak valid → load returns .bak content + warning logged.
    Evidence: .sisyphus/evidence/task-4-backup.log

  Scenario: Schema version rejection
    Tool: Bash
    Preconditions: T1+T2+T4 complete
    Steps:
      1. cargo test state::tests::unsupported_version -- --nocapture
    Expected Result: file with "version": 99 returns Err(StateError::UnsupportedVersion(99))
    Evidence: .sisyphus/evidence/task-4-version.log
  ```

  **Commit**: YES — `feat(state): schema v1 + atomic persistence`

- [x] 5. **Config paths + XDG fallback**

  **What to do**:
  - Em `src/config.rs`:
    - `pub fn repo_state_path(repo_root: &Path) -> PathBuf`: retorna `repo_root/.martins/state.json`
    - `pub fn repo_state_path_with_fallback(repo_root: &Path) -> PathBuf`: se `repo_root/.martins/` não é writable → fallback para `{data_dir}/martins/{hash_of_repo_path}/state.json` usando crate `directories` (`ProjectDirs`)
    - `pub fn is_writable(path: &Path) -> bool`: tenta criar `.martins/.write_probe`, deleta
    - `pub fn hash_repo_path(p: &Path) -> String`: SHA-256 truncado a 12 chars hex (usando `sha2` já em deps de T1)
  - Logging: quando cai em fallback, `tracing::warn!("repo .martins/ not writable, using XDG fallback at {}")`

  **Must NOT do**:
  - NÃO tentar escrever em `~/.config/` (usar `directories::ProjectDirs::data_dir()` canônico)
  - NÃO criar diretórios cegamente (probe primeiro)

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T3, T4, T6, T7
  - **Parallel Group**: Wave 1
  - **Blocks**: T4 (consome), T6
  - **Blocked By**: T1

  **References**:
  - `directories` crate docs: https://docs.rs/directories/latest/directories/
  - XDG spec: https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html
  - Pattern: https://github.com/atuinsh/atuin/blob/main/crates/atuin-common/src/utils.rs — hash of path for data dir

  **WHY Each Reference Matters**:
  - `directories`: abstrai macOS vs Linux data dirs (`~/Library/Application Support` vs `~/.local/share`)
  - atuin utils: exatamente o padrão hash_of_path usado em múltiplos tools Rust

  **Acceptance Criteria**:
  - [ ] `repo_state_path(TempDir::new())` retorna `{temp}/.martins/state.json`
  - [ ] `is_writable(temp)` → true; `is_writable("/")` → false em Linux/macOS non-root
  - [ ] `hash_repo_path` é determinístico (mesmo input → mesmo output) e 12 chars hex
  - [ ] `repo_state_path_with_fallback(read-only dir)` retorna path em `{data_dir}/martins/{hash}/state.json`

  **QA Scenarios**:

  ```
  Scenario: Normal writable repo uses .martins/
    Tool: Bash
    Preconditions: T1+T5 complete
    Steps:
      1. cargo test config::tests::writable_repo_uses_local -- --nocapture
    Expected Result: TempDir is writable → config returns temp/.martins/state.json (not XDG)
    Evidence: .sisyphus/evidence/task-5-local.log

  Scenario: Read-only repo falls back to XDG data dir
    Tool: Bash
    Preconditions: T1+T5 complete, permission to chmod
    Steps:
      1. cargo test config::tests::readonly_fallback -- --nocapture
    Expected Result: chmod 555 dir → config returns {data_dir}/martins/{hash}/state.json with tracing warn
    Evidence: .sisyphus/evidence/task-5-fallback.log
  ```

  **Commit**: YES — `feat(config): resolve per-repo and xdg paths`

- [x] 6. **Pre-flight binary detection + install command mapping**

  **What to do**:
  - Em `src/tools.rs`:
    - `pub enum Tool { Bat, Opencode, Claude, Codex }` + `impl Tool { fn binary_name(&self) -> &str }`
    - `pub fn detect(tool: Tool) -> Option<PathBuf>`: usa `which` crate (já em deps de T1) para localizar binário
    - `pub struct MissingTools { pub tools: Vec<Tool> }` + `pub fn preflight() -> MissingTools`
    - `pub fn install_command(tool: Tool) -> Option<InstallCmd>`:
      - Detecta OS via `std::env::consts::OS`
      - macOS + brew disponível: `InstallCmd { program: "brew", args: vec!["install", "bat"] }` (e análogos para opencode via homebrew tap, claude via npm, codex via npm)
      - Linux + apt: `apt install bat`; opencode/claude/codex via `curl | bash` ou `npm install -g`
      - Fallback: `cargo install bat` para bat
      - Se nenhum gerenciador → retorna None com instrução manual
    - `pub fn run_install(cmd: InstallCmd) -> Result<()>`: spawn em foreground, stream stdout; retorna ao completar
  - Opencode install command: do official docs https://opencode.ai — buscar comando canônico

  **Must NOT do**:
  - NÃO rodar install sem confirmação explícita (isso é responsabilidade do T19)
  - NÃO tentar detectar arch/distro além de OS (macOS/Linux)
  - NÃO fazer install silenciosamente em background

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high` — Precisa pesquisar comandos exatos de install para cada ferramenta
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T3, T4, T5, T7
  - **Parallel Group**: Wave 1
  - **Blocks**: T19, T23
  - **Blocked By**: T1, T5

  **References**:
  - `which` crate: https://docs.rs/which/latest/which/
  - bat install: https://github.com/sharkdp/bat#installation
  - opencode install: https://opencode.ai (oficial)
  - Pattern: https://github.com/rust-lang/rustup/blob/master/src/cli/self_update.rs — pattern de detecção de package manager

  **WHY Each Reference Matters**:
  - `which` é a crate canônica pra localizar binários (mesma semântica do shell `which`)
  - bat README lista comandos oficiais de instalação por SO
  - rustup self_update: padrão robusto de detectar e priorizar gerenciadores

  **Acceptance Criteria**:
  - [ ] `detect(Tool::Bat)` em sistema com bat → `Some(path)`
  - [ ] `detect(Tool::Opencode)` em sistema sem opencode → `None`
  - [ ] `preflight()` retorna lista de todos os tools ausentes
  - [ ] `install_command(Tool::Bat)` em macOS (mockado) → `Some(InstallCmd { "brew", ["install", "bat"] })`
  - [ ] `install_command(Tool::Bat)` em Linux (mockado) → `Some(InstallCmd { "apt", ["install", "-y", "bat"] })` OU fallback cargo

  **QA Scenarios**:

  ```
  Scenario: Detects installed bat binary
    Tool: Bash
    Preconditions: T1+T6 complete. bat installed (`which bat` succeeds)
    Steps:
      1. cargo test tools::tests::detect_bat -- --nocapture
    Expected Result: returns Some(PathBuf) with valid path. Verified via is_file() check.
    Evidence: .sisyphus/evidence/task-6-detect.log

  Scenario: Reports missing opencode
    Tool: Bash
    Preconditions: T1+T6 complete. opencode NOT installed.
    Steps:
      1. # Test impl uses a mocked PATH environment injected via std::env::set_var inside the test itself (not via shell),
      2. # OR uses the `which::which_in` API to search a custom PATH passed as argument.
      3. # Either way, `cargo` runs with full ambient PATH; only the lookup inside the test is scoped.
      4. cargo test tools::tests::missing_opencode -- --nocapture
    Expected Result: Test sets an internal "empty-ish" PATH (e.g. just /tmp) when calling detect(). detect(Opencode) returns None. preflight() includes Tool::Opencode. Ambient cargo invocation is unaffected.
    Evidence: .sisyphus/evidence/task-6-missing.log

  Scenario: Install command resolution per OS
    Tool: Bash
    Preconditions: T1+T6 complete
    Steps:
      1. cargo test tools::tests::install_cmd_macos -- --nocapture
      2. cargo test tools::tests::install_cmd_linux -- --nocapture
    Expected Result: OS detection returns correct InstallCmd struct for each tool.
    Evidence: .sisyphus/evidence/task-6-install-cmd.log
  ```

  **Commit**: YES — `feat(tools): pre-flight binary detection and install command mapping`

- [x] 7. **Logging / tracing setup**

  **What to do**:
  - Em `src/main.rs`: init `tracing_subscriber` com `EnvFilter::from_default_env()`
  - Default level: `info` em release, `debug` em debug build
  - Logs em arquivo: `.martins/logs/martins-{date}.log` com rotação diária via `tracing-appender` (já em deps de T1)
  - Quando TUI está ativo, desabilitar console output (só arquivo) — senão corrompe ratatui
  - `tracing::info!("starting martins v{}", env!("CARGO_PKG_VERSION"))` no start
  - Panic hook: captura panic → loga via tracing → restaura terminal antes de crashar

  **Must NOT do**:
  - NÃO logar em stdout/stderr durante TUI (corrompe ratatui)
  - NÃO logar conteúdo de arquivos (vazamento)
  - NÃO usar `println!`/`eprintln!` no código (sempre tracing)

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T3, T4, T5, T6
  - **Parallel Group**: Wave 1
  - **Blocks**: none (mas todos consomem logging)
  - **Blocked By**: T1

  **References**:
  - tracing-subscriber docs: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/
  - tracing-appender: https://docs.rs/tracing-appender/latest/tracing_appender/
  - Pattern: https://github.com/ratatui-org/templates/blob/main/async/src/logging.rs — logging em TUI

  **WHY Each Reference Matters**:
  - ratatui async template tem o pattern exato: file-only logging quando TUI está ativo
  - tracing-appender fornece `rolling::daily` out-of-the-box

  **Acceptance Criteria**:
  - [ ] `martins` startup escreve em `.martins/logs/martins-YYYY-MM-DD.log`
  - [ ] TUI não exibe logs no stdout
  - [ ] Panic em TUI → log escrito → terminal restaurado (sem raw mode lock)
  - [ ] `RUST_LOG=debug martins` aumenta verbosity

  **QA Scenarios**:

  ```
  Scenario: Log subscriber initializes and writes to rolling file
    Tool: Bash
    Preconditions: T1+T7 complete
    Steps:
      1. cargo test logging::tests::init_creates_rolling_file -- --nocapture
    Expected Result: Unit test uses tempfile::TempDir as logs dir. Calls init_logging(&tmp_path). Emits tracing::info!("starting martins v0.1.0"). Assertion 1: tmp_path/martins-YYYY-MM-DD.log exists. Assertion 2: file content contains "starting martins v0.1.0". Assertion 3: stdout has zero lines (file-only routing).
    Evidence: .sisyphus/evidence/task-7-init.log

  Scenario: RUST_LOG env filter is respected
    Tool: Bash
    Preconditions: T1+T7 complete
    Steps:
      1. cargo test logging::tests::respects_rust_log_env -- --nocapture
    Expected Result: Test sets env RUST_LOG=debug via std::env::set_var before init. Emits tracing::debug!("dbg-line"). File contains "dbg-line". When set to "warn", debug line NOT in file.
    Evidence: .sisyphus/evidence/task-7-env.log

  Scenario: Panic hook calls terminal restoration before unwinding
    Tool: Bash
    Preconditions: T1+T7 complete
    Steps:
      1. cargo test logging::tests::panic_hook_restores_terminal -- --nocapture
    Expected Result: Test installs panic hook with an AtomicBool flag that flips in the hook. catch_unwind(|| panic!("x")). Assertion: flag flipped before error propagates. Log file contains panic message.
    Evidence: .sisyphus/evidence/task-7-panic.log
  ```

  **Commit**: YES — `chore(log): tracing setup with file rotation and panic hook`

### Wave 2 — Domain + Infrastructure (T8-T14, paralelizáveis)

- [x] 8. **Input modes Normal/Terminal + keymap registry (foundation)**

  **What to do**:
  - Em `src/keys.rs`:
    - `pub enum InputMode { Normal, Terminal }` já definido em T2
    - `pub enum Action { FocusLeft, FocusRight, FocusTerminal, NextItem, PrevItem, EnterSelected, NewWorkspace, NewWorkspaceAuto, ArchiveWorkspace, UnarchiveWorkspace, DeleteWorkspace, NewTab, CloseTab, SwitchTab(u8), Quit, ToggleSidebarLeft, ToggleSidebarRight, OpenFuzzy, EnterTerminalMode, ExitTerminalMode, ShowHelp, Preview, Edit }`
    - `pub struct Keymap { normal: HashMap<KeyEvent, Action>, terminal: HashMap<KeyEvent, Action> }`
    - `impl Keymap { pub fn default() -> Self }` com bindings do draft (n/N/d/a/u/j/k/h/l/Tab/Enter/p/e/t/T/1-9/Ctrl+B/?/q/i/Esc/Esc)
    - `pub fn resolve(mode: InputMode, key: KeyEvent) -> Option<Action>`
    - No modo Terminal: SÓ `Ctrl+B` (ou `Esc Esc` com double-tap timeout 300ms) dispara `ExitTerminalMode`; todas as outras teclas retornam `None` (serão forwardadas pro PTY por quem chama)
    - No modo Normal: `i` dispara `EnterTerminalMode` quando o pane focado é o terminal central (senão ignora)

  **Must NOT do**:
  - NÃO hardcodar keybindings em outros módulos (sempre via Keymap)
  - NÃO capturar Ctrl+C em modo Terminal (deve ir pro PTY para interromper agente)
  - NÃO suportar remapping custom no MVP (só defaults)

  **Recommended Agent Profile**:
  - **Category**: `deep` — Decisões modais complexas, timing de double-tap, edge cases de input
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T9-T14
  - **Parallel Group**: Wave 2
  - **Blocks**: T15, T18, T22
  - **Blocked By**: T2

  **References**:
  - Pattern: https://github.com/vercel/turborepo/blob/main/crates/turborepo-ui/src/tui/input.rs — modal input exactly this style
  - Pattern: https://github.com/helix-editor/helix/blob/master/helix-term/src/keymap.rs — keymap as HashMap
  - Crossterm KeyEvent: https://docs.rs/crossterm/latest/crossterm/event/struct.KeyEvent.html

  **WHY Each Reference Matters**:
  - turborepo/input.rs: exatamente o padrão que queremos (Normal/Terminal modes com Ctrl-Z toggle)
  - helix keymap: como estruturar `HashMap<KeyEvent, Action>` de forma extensível
  - crossterm docs: API exata para KeyEvent matching (não confundir KeyCode vs KeyEvent)

  **Acceptance Criteria**:
  - [ ] `resolve(Normal, 'j')` → `Some(NextItem)`
  - [ ] `resolve(Normal, 'i')` → `Some(EnterTerminalMode)` (mesmo sem pane context aqui; context gating é em T18/T22)
  - [ ] `resolve(Terminal, 'j')` → `None` (forward pro PTY)
  - [ ] `resolve(Terminal, Ctrl+B)` → `Some(ExitTerminalMode)`
  - [ ] `resolve(Terminal, Ctrl+C)` → `None` (forward pro PTY, NÃO deve sair)
  - [ ] `resolve(Normal, Ctrl+Q)` → `Some(Quit)`

  **QA Scenarios**:

  ```
  Scenario: Normal mode navigation keys map to actions
    Tool: Bash
    Preconditions: T2+T8 complete
    Steps:
      1. cargo test keys::tests::normal_mode_mappings -- --nocapture
    Expected Result: j→NextItem, k→PrevItem, Tab→cycle focus, n→NewWorkspace, q→Quit
    Evidence: .sisyphus/evidence/task-8-normal.log

  Scenario: Terminal mode forwards printable keys to PTY
    Tool: Bash
    Preconditions: T2+T8 complete
    Steps:
      1. cargo test keys::tests::terminal_mode_forwards -- --nocapture
    Expected Result: Keys like 'a', 'j', Enter, Backspace, Ctrl+C all return None (to be sent to PTY). Only Ctrl+B returns Some(ExitTerminalMode).
    Evidence: .sisyphus/evidence/task-8-terminal.log

  Scenario: Double-Esc exits Terminal mode within 300ms window
    Tool: Bash
    Preconditions: T2+T8 complete
    Steps:
      1. cargo test keys::tests::double_esc_exits -- --nocapture
    Expected Result: First Esc at t=0 returns None (buffered). Second Esc at t=200ms returns Some(ExitTerminalMode). Second Esc at t=500ms returns None (too late) + first Esc forwarded to PTY.
    Evidence: .sisyphus/evidence/task-8-double-esc.log
  ```

  **Commit**: YES — `feat(keys): normal/terminal input modes with keymap registry`

- [x] 9. **Git repo ops: open, discover, HEAD, base branch capture**

  **What to do**:
  - Em `src/git/repo.rs`:
    - `pub fn discover(start: &Path) -> Result<PathBuf>`: usa `git2::Repository::discover` para achar repo raiz subindo pastas
    - `pub fn open(path: &Path) -> Result<Repository>`: wrapper trivial, consistent error type
    - `pub fn current_branch(repo: &Repository) -> Result<String>`: lê HEAD → se symbolic, extrai shorthand; se detached, retorna commit hash truncado
    - `pub async fn current_branch_async(path: PathBuf) -> Result<String>`: wraps `spawn_blocking` (pattern canônico que todos os git ops usam)
    - `pub fn is_bare(repo: &Repository) -> bool`: detect bare repos (não suportar, erro)
    - `pub fn main_repo_root(worktree_path: &Path) -> Result<PathBuf>`: se path é worktree, resolve pra common dir (libgit2 API: `repo.commondir()`)
  - Todas as funções async **abrem `Repository` fresh** dentro do `spawn_blocking` (NÃO compartilhar)

  **Must NOT do**:
  - NÃO compartilhar `Repository` via `Arc<Mutex<>>` (fresh open per op)
  - NÃO chamar git2 fora de `spawn_blocking` em código async
  - NÃO suportar bare repos (sair com erro claro)

  **Recommended Agent Profile**:
  - **Category**: `deep` — API libgit2 tem edge cases (detached HEAD, submodules, worktrees)
  - **Skills**: `[git-master]` — domínio overlap com git ops

  **Parallelization**:
  - **Can Run In Parallel**: YES com T8, T10-T14
  - **Parallel Group**: Wave 2
  - **Blocks**: T10, T13
  - **Blocked By**: T2

  **References**:
  - git2 docs: https://docs.rs/git2/latest/git2/struct.Repository.html#method.discover
  - git2 docs: https://docs.rs/git2/latest/git2/struct.Repository.html#method.commondir
  - Pattern: https://github.com/martinvonz/jj/blob/main/lib/src/git_backend.rs — JJ's git2 wrappers (production-grade)
  - libgit2 worktree docs: https://libgit2.org/docs/reference/main/worktree/git_worktree_list.html

  **WHY Each Reference Matters**:
  - JJ (Jujutsu): referência mais robusta de git2 em Rust, trata todos edge cases
  - commondir API é CRÍTICA: `state.json` vive no main repo, mesmo se martins for lançado de um worktree

  **Acceptance Criteria**:
  - [ ] `discover(sub_dir_of_repo)` → retorna repo root
  - [ ] `discover(non_repo_dir)` → `Err(GitError::NotARepository)`
  - [ ] `current_branch` em detached HEAD → retorna hash truncado (8 chars)
  - [ ] `main_repo_root(worktree_path)` → retorna main repo path
  - [ ] `is_bare(bare_repo)` → `true`
  - [ ] All async fns run git2 in spawn_blocking (verified via tokio console or explicit assertion)

  **QA Scenarios**:

  ```
  Scenario: Repo discovery from nested path
    Tool: Bash
    Preconditions: T1+T9 complete
    Steps:
      1. cargo test git::repo::tests::discover_nested -- --nocapture
    Expected Result: TempDir with git init + nested dir "a/b/c"; discover from "a/b/c" returns TempDir path.
    Evidence: .sisyphus/evidence/task-9-discover.log

  Scenario: Detached HEAD returns commit hash
    Tool: Bash
    Preconditions: T1+T9 complete
    Steps:
      1. cargo test git::repo::tests::detached_head -- --nocapture
    Expected Result: repo with detached HEAD (checkout commit directly) → current_branch returns 8-char hex.
    Evidence: .sisyphus/evidence/task-9-detached.log

  Scenario: Worktree main repo resolution
    Tool: Bash
    Preconditions: T1+T9 complete
    Steps:
      1. cargo test git::repo::tests::worktree_main_repo -- --nocapture
    Expected Result: TempDir with main + worktree; main_repo_root(worktree_path) returns main path.
    Evidence: .sisyphus/evidence/task-9-main-repo.log
  ```

  **Commit**: YES — `feat(git): repo discovery and base branch capture`

- [x] 10. **Git worktree CRUD via git2**

  **What to do**:
  - Em `src/git/worktree.rs`:
    - `pub async fn list(repo_path: PathBuf) -> Result<Vec<WorktreeInfo>>`: `spawn_blocking` → `Repository::open` → `worktrees()` → map para `WorktreeInfo { name, path, branch }`
    - `pub async fn create(repo_path: PathBuf, name: String, base_branch: String) -> Result<PathBuf>`:
      - `spawn_blocking`
      - Abre repo, valida nome (usa `src/mpb.rs::validate`)
      - Worktree path: `{repo_parent}/{repo_name}-{name}` (irmão do repo)
      - Resolve base_branch commit
      - Cria nova branch `{name}` apontando pro commit da base
      - `repo.worktree(name, &worktree_path, Some(&WorktreeAddOptions))` com `reference` = nova branch
      - Retorna worktree path absoluto
    - `pub async fn prune(repo_path: PathBuf, name: String, delete_branch: bool) -> Result<()>`:
      - `spawn_blocking`
      - Abre main repo, acha worktree por nome, `.prune()` (libgit2 remove .git/worktrees/{name} entries)
      - Remove diretório do worktree no filesystem (rm -rf path — usar `std::fs::remove_dir_all`)
      - Se `delete_branch` → acha branch `{name}`, deleta (`branch.delete()`)
    - `pub async fn count_unpushed_commits(repo_path: PathBuf, worktree_name: String, base_branch: String) -> Result<usize>`:
      - `spawn_blocking`
      - Abre worktree repo, `revwalk`: `base_branch..HEAD`, conta

  **Must NOT do**:
  - NÃO implementar merge/rebase/checkout além de create (apenas CRUD)
  - NÃO deletar branch automaticamente em prune (sempre opt-in)
  - NÃO suportar worktree em paths arbitrários (sempre sibling do repo)

  **Recommended Agent Profile**:
  - **Category**: `deep` — Worktree API de libgit2 tem armadilhas (lock files, dangling refs)
  - **Skills**: `[git-master]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T8, T9, T11, T12, T13, T14
  - **Parallel Group**: Wave 2
  - **Blocks**: T16, T19, T22
  - **Blocked By**: T2, T4, T9

  **References**:
  - git2 Worktree: https://docs.rs/git2/latest/git2/struct.Worktree.html
  - git2 WorktreeAddOptions: https://docs.rs/git2/latest/git2/struct.WorktreeAddOptions.html
  - Pattern: https://github.com/martinvonz/jj/blob/main/lib/src/git_backend.rs — worktree ops em JJ
  - Pattern: https://github.com/ratatui-org/templates/blob/main/async/ — spawn_blocking pattern
  - libgit2 git_revwalk: https://libgit2.org/docs/reference/main/revwalk/git_revwalk_push_range.html

  **WHY Each Reference Matters**:
  - git2 WorktreeAddOptions: API para passar `reference` (branch target) — se omitido, libgit2 cria branch com o nome do worktree que pode colidir
  - revwalk com `base..HEAD`: API canônica para contar commits "à frente" de uma branch

  **Acceptance Criteria**:
  - [ ] `create` cria worktree em sibling dir, nova branch, retorna path
  - [ ] `list` retorna todos os worktrees existentes
  - [ ] `prune(name, delete_branch=false)` remove worktree mas preserva branch
  - [ ] `prune(name, delete_branch=true)` remove worktree + branch
  - [ ] `count_unpushed_commits` retorna N após N commits na worktree
  - [ ] `create` com nome inválido (via `mpb::validate`) → `Err`
  - [ ] `create` com nome já existente → `Err(WorktreeError::NameExists)`

  **QA Scenarios**:

  ```
  Scenario: Create worktree with custom name
    Tool: Bash
    Preconditions: T1+T4+T9+T10 complete
    Steps:
      1. cargo test git::worktree::tests::create_custom -- --nocapture
    Expected Result: TempDir repo + 1 initial commit on main. create("repo", "caetano", "main") creates sibling dir "repo-caetano" with branch "caetano", pointing at main HEAD.
    Evidence: .sisyphus/evidence/task-10-create.log

  Scenario: Prune preserves branch when delete_branch=false
    Tool: Bash
    Preconditions: T1+T4+T9+T10 complete
    Steps:
      1. cargo test git::worktree::tests::prune_preserves -- --nocapture
    Expected Result: After prune, worktree dir removed, git branch list still contains "caetano".
    Evidence: .sisyphus/evidence/task-10-prune.log

  Scenario: Count unpushed commits
    Tool: Bash
    Preconditions: T1+T4+T9+T10 complete
    Steps:
      1. cargo test git::worktree::tests::count_ahead -- --nocapture
    Expected Result: Create worktree "x" from "main". Add 3 commits in worktree. count_unpushed_commits returns 3.
    Evidence: .sisyphus/evidence/task-10-count.log

  Scenario: Error on duplicate name
    Tool: Bash
    Preconditions: T1+T4+T9+T10 complete
    Steps:
      1. cargo test git::worktree::tests::duplicate_name -- --nocapture
    Expected Result: First create succeeds; second create with same name returns Err(NameExists).
    Evidence: .sisyphus/evidence/task-10-dup.log
  ```

  **Commit**: YES — `feat(git): worktree crud via git2`

- [x] 11. **PTY session lifecycle: spawn, read loop, EOF, resize, kill**

  **What to do**:
  - Em `src/pty/session.rs`:
    - `pub struct PtySession { id: u64, parser: Arc<RwLock<vt100::Parser>>, master: Box<dyn MasterPty + Send>, child: Box<dyn Child + Send + Sync>, writer: Box<dyn Write + Send>, status: Arc<Mutex<PtyStatus>>, exit_rx: oneshot::Receiver<i32> }`
    - `pub enum PtyStatus { Running, Exited(i32) }`
    - `pub fn spawn(cwd: PathBuf, program: &str, args: &[String], rows: u16, cols: u16) -> Result<PtySession>`:
      - `portable_pty::native_pty_system().openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })`
      - `CommandBuilder::new(program).args(args).cwd(cwd)`
      - `pair.slave.spawn_command(cmd)` → child
      - Spawn `spawn_blocking` thread que faz `loop { reader.read() → if 0 bytes → send exit code via oneshot → break; else parser.write().unwrap().process(&buf[..n]) }`
      - `parser` criado com scrollback 1000 linhas: `vt100::Parser::new(rows, cols, 1000)`
      - Retorna `PtySession`
    - `pub fn write_input(&self, data: &[u8]) -> Result<()>`: escreve no writer
    - `pub fn resize(&self, rows: u16, cols: u16) -> Result<()>`: `master.resize(PtySize{...})` + `parser.write().set_size(rows, cols)`
    - `pub fn kill(&mut self) -> Result<()>`: `child.kill()` (SIGHUP via portable-pty); NÃO chamar `wait()` aqui (blocking)
    - `impl Drop`: tenta kill se ainda running (best-effort)

  **Must NOT do**:
  - NÃO ler `master` no main event loop (só `spawn_blocking`)
  - NÃO chamar `child.wait()` bloqueante fora de thread dedicada
  - NÃO compartilhar `vt100::Parser` entre sessões
  - NÃO fazer parsing manual de ANSI (vt100 faz tudo)

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain` — Sincronização complexa: sync read blocking + async main loop + Drop safety + resize race conditions
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T8, T9, T10, T12, T13, T14
  - **Parallel Group**: Wave 2
  - **Blocks**: T18, T22
  - **Blocked By**: T2

  **References**:
  - tui-term nested_shell_async example: https://github.com/a-kenji/tui-term/blob/main/examples/nested_shell_async.rs — PATTERN EXATO
  - portable-pty docs: https://docs.rs/portable-pty/latest/portable_pty/
  - vt100 docs: https://docs.rs/vt100/latest/vt100/
  - Production reference: https://github.com/vercel/turborepo/blob/main/crates/turborepo-ui/src/tui/pane.rs — PTY wrapping in turborepo

  **WHY Each Reference Matters**:
  - tui-term nested_shell_async: template canônico para Arc<RwLock<Parser>> + spawn_blocking read loop
  - portable-pty: API exata de `openpty`, `spawn_command`, `kill`, `resize`
  - turborepo pane.rs: production-grade com resize handling, EOF detection, shutdown

  **Acceptance Criteria**:
  - [ ] `spawn("bash", ["-c", "echo hello; exit 42"])` → parser eventualmente contém "hello" → status = Exited(42)
  - [ ] `spawn(...).resize(30, 100)` → `parser.size()` == (30, 100)
  - [ ] `write_input(b"ls\n")` forwards to child (verified by capturing output)
  - [ ] `kill()` mata child em <100ms em child sleep-infinito
  - [ ] Scrollback limitado a 1000: após 2000 linhas, parser só mantém 1000

  **QA Scenarios**:

  ```
  Scenario: Spawn and capture echo output
    Tool: Bash
    Preconditions: T1+T11 complete
    Steps:
      1. cargo test pty::session::tests::spawn_echo -- --nocapture
    Expected Result: spawn("/bin/echo", ["hello"]) → wait for exit via oneshot → parser.screen().contents() contains "hello". Exit code 0.
    Evidence: .sisyphus/evidence/task-11-echo.log

  Scenario: EOF detection on child exit with nonzero code
    Tool: Bash
    Preconditions: T1+T11 complete
    Steps:
      1. cargo test pty::session::tests::eof_exit_code -- --nocapture
    Expected Result: spawn("/bin/sh", ["-c", "exit 42"]) → within 2s, status becomes Exited(42).
    Evidence: .sisyphus/evidence/task-11-eof.log

  Scenario: Resize propagates to parser and master
    Tool: Bash
    Preconditions: T1+T11 complete
    Steps:
      1. cargo test pty::session::tests::resize -- --nocapture
    Expected Result: After resize(40, 120), parser.screen().size() == (40, 120). Child receives SIGWINCH (verified by spawning 'tput cols' after resize returns 120).
    Evidence: .sisyphus/evidence/task-11-resize.log

  Scenario: Kill terminates hung process
    Tool: Bash
    Preconditions: T1+T11 complete
    Steps:
      1. cargo test pty::session::tests::kill_hang -- --nocapture
    Expected Result: spawn("/bin/sh", ["-c", "sleep 3600"]) → kill() → status Exited(negative or SIGHUP code) within 200ms.
    Evidence: .sisyphus/evidence/task-11-kill.log
  ```

  **Commit**: YES — `feat(pty): session lifecycle with eof detection and resize`

- [x] 12. **File watcher: notify + debouncer with filter**

  **What to do**:
  - Em `src/watcher.rs`:
    - `pub struct Watcher { debouncer: Debouncer<...>, events_rx: tokio::sync::mpsc::UnboundedReceiver<FsEvent> }`
    - `pub enum FsEvent { Changed(PathBuf), Removed(PathBuf) }`
    - `pub fn new() -> Result<Self>`: `notify_debouncer_mini::new_debouncer(Duration::from_millis(750), callback)` — callback envia via channel tokio
    - `pub fn watch(&mut self, path: &Path) -> Result<()>`: `debouncer.watcher().watch(path, RecursiveMode::Recursive)`
    - `pub fn unwatch(&mut self, path: &Path) -> Result<()>`
    - Filtro **na callback, não no recv**: ignora paths contendo `/.git/`, `/target/`, `/node_modules/`, `/.martins/`, `/dist/`, `/build/`, `/.next/`, `/.venv/`
    - `pub async fn next_event(&mut self) -> Option<FsEvent>`

  **Must NOT do**:
  - NÃO usar `PollWatcher` fallback (usar sempre native)
  - NÃO watch recursivamente o repo root diretamente (watch cada worktree individualmente)
  - NÃO filtrar no recv side (latência)

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high` — File watching tem platform-specific quirks (FSEvents coalescing no macOS)
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T8, T9, T10, T11, T13, T14
  - **Parallel Group**: Wave 2
  - **Blocks**: T13, T17, T22
  - **Blocked By**: T2

  **References**:
  - notify docs: https://docs.rs/notify/latest/notify/
  - notify-debouncer-mini: https://docs.rs/notify-debouncer-mini/latest/
  - Pattern: https://github.com/cargo-bins/cargo-binstall/blob/main/crates/binstalk-downloader/src/remote.rs — tokio async wrapping
  - Issue on macOS FSEvents quirks: https://github.com/notify-rs/notify/issues/403

  **WHY Each Reference Matters**:
  - notify 8.x mudou API significativamente (antes era v6); docs são específicas pra v8
  - debouncer-mini é pattern canônico para evitar storm de eventos em saves grandes (VSCode faz multi-write)
  - macOS FSEvents coalesce: debouncer cobre, mas documentar

  **Acceptance Criteria**:
  - [ ] Watch dir → write file → recv `FsEvent::Changed` dentro de 1.5s (debounce 750ms)
  - [ ] Delete file → recv `FsEvent::Removed`
  - [ ] Write em `.git/HEAD` → NÃO gera evento (filtrado)
  - [ ] Write em `target/debug/foo` → NÃO gera evento
  - [ ] Multiple writes em 500ms → 1 evento (debounced)

  **QA Scenarios**:

  ```
  Scenario: Detects file modification after debounce window
    Tool: Bash
    Preconditions: T1+T12 complete
    Steps:
      1. cargo test watcher::tests::detect_change -- --nocapture
    Expected Result: TempDir watched. Write file. Receive FsEvent::Changed within 1500ms.
    Evidence: .sisyphus/evidence/task-12-change.log

  Scenario: Filters .git and target directories
    Tool: Bash
    Preconditions: T1+T12 complete
    Steps:
      1. cargo test watcher::tests::filter_noise -- --nocapture
    Expected Result: TempDir watched. Write to TempDir/.git/HEAD and TempDir/target/x. No events received within 2s timeout.
    Evidence: .sisyphus/evidence/task-12-filter.log

  Scenario: Debounces rapid writes to single event
    Tool: Bash
    Preconditions: T1+T12 complete
    Steps:
      1. cargo test watcher::tests::debounce_rapid -- --nocapture
    Expected Result: Write to same file 10 times with 50ms interval (500ms total). Receive exactly 1 FsEvent::Changed within 2s.
    Evidence: .sisyphus/evidence/task-12-debounce.log
  ```

  **Commit**: YES — `feat(watcher): file events with debouncer and filters`

- [x] 13. **Git diff vs base branch — modified file list**

  **What to do**:
  - Em `src/git/diff.rs`:
    - `pub enum FileStatus { Modified, Added, Deleted, Renamed, Untracked }`
    - `pub struct FileEntry { path: PathBuf, status: FileStatus }`
    - `pub async fn modified_files(worktree_path: PathBuf, base_branch: String) -> Result<Vec<FileEntry>>`:
      - `spawn_blocking`
      - Abre worktree repo fresh
      - Resolve `base_branch` to commit
      - `diff_tree_to_workdir_with_index` entre base_tree e workdir
      - Mapeia cada delta → FileEntry
      - Também lista untracked: `status_file()` com `include_untracked = true`
      - Ordena: untracked primeiro (usuário quer ver o que é novo), depois modified/added/deleted por path
    - `pub async fn is_binary(worktree_path: PathBuf, relative_path: PathBuf) -> Result<bool>`: usa `git2::Repository::blob_path` + heurística (`blob.is_binary()`)

  **Must NOT do**:
  - NÃO cachear resultado (sempre fresh — o watcher decide quando refazer)
  - NÃO mostrar conteúdo do diff (só lista de arquivos + status)
  - NÃO aplicar ignore globs próprios (respeitar .gitignore via libgit2 status API)

  **Recommended Agent Profile**:
  - **Category**: `deep` — DiffOptions tem muitas flags, ordem importa para corretness
  - **Skills**: `[git-master]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T8, T9, T10, T11, T12, T14
  - **Parallel Group**: Wave 2
  - **Blocks**: T17
  - **Blocked By**: T9, T12

  **References**:
  - git2 Diff: https://docs.rs/git2/latest/git2/struct.Diff.html
  - git2 Statuses: https://docs.rs/git2/latest/git2/struct.Statuses.html
  - Pattern: https://github.com/extrawurst/gitui/blob/master/asyncgit/src/sync/diff.rs — gitui's exact use case
  - libgit2 status example: https://libgit2.org/docs/examples/status/

  **WHY Each Reference Matters**:
  - gitui asyncgit/diff.rs: exatamente o que queremos (diff listing em TUI async)
  - libgit2 status example: mostra como combinar untracked + modified corretamente

  **Acceptance Criteria**:
  - [ ] Worktree sem mudanças → `modified_files` retorna `vec![]`
  - [ ] Modificar arquivo tracked → FileEntry com status Modified
  - [ ] Criar arquivo novo → FileEntry com status Untracked
  - [ ] Deletar arquivo → FileEntry com status Deleted
  - [ ] `.git/` e ignored → não aparece
  - [ ] Base branch inexistente → `Err(DiffError::BaseBranchMissing)`

  **QA Scenarios**:

  ```
  Scenario: Empty diff on fresh worktree
    Tool: Bash
    Preconditions: T1+T9+T13 complete
    Steps:
      1. cargo test git::diff::tests::empty_worktree -- --nocapture
    Expected Result: Worktree created from main, no modifications. modified_files returns empty Vec.
    Evidence: .sisyphus/evidence/task-13-empty.log

  Scenario: Full status coverage
    Tool: Bash
    Preconditions: T1+T9+T13 complete
    Steps:
      1. cargo test git::diff::tests::full_coverage -- --nocapture
    Expected Result: Worktree with 1 modified + 1 added + 1 deleted + 1 untracked + 1 in .gitignore. modified_files returns exactly 4 entries (ignored excluded), correct statuses.
    Evidence: .sisyphus/evidence/task-13-coverage.log

  Scenario: Missing base branch error
    Tool: Bash
    Preconditions: T1+T9+T13 complete
    Steps:
      1. cargo test git::diff::tests::missing_base -- --nocapture
    Expected Result: modified_files with base="doesnt-exist" returns Err(BaseBranchMissing).
    Evidence: .sisyphus/evidence/task-13-missing-base.log
  ```

  **Commit**: YES — `feat(git): diff vs base branch with file status list`

- [x] 14. **.gitignore auto-edit + first-run bootstrap**

  **What to do**:
  - Em `src/config.rs` (expand):
    - `pub fn ensure_gitignore(repo_root: &Path) -> Result<GitignoreAction>`:
      - `pub enum GitignoreAction { NoChange, Appended, Created }`
      - Se `{repo}/.gitignore` não existe → cria com conteúdo `.martins/\n`, retorna `Created`
      - Se existe e já contém linha `.martins/` ou `/.martins/` (regex `^/?\.martins/?$`) → `NoChange`
      - Se existe mas não tem → append `\n.martins/\n`, retorna `Appended`
  - Chamar `ensure_gitignore` no bootstrap da aplicação antes de criar qualquer worktree
  - Em caso de `Created`/`Appended`: `tracing::info!("added .martins/ to .gitignore")`

  **Must NOT do**:
  - NÃO modificar `.gitignore` sem needing to (verificar primeiro)
  - NÃO usar regex complexo (simples contains por linha)
  - NÃO tocar outros arquivos do repo (nunca)

  **Recommended Agent Profile**:
  - **Category**: `quick` — Lógica simples de append condicional
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T8-T13
  - **Parallel Group**: Wave 2
  - **Blocks**: T22
  - **Blocked By**: T4, T9

  **References**:
  - Rust `std::fs::OpenOptions::append`: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html
  - Pattern: https://github.com/rust-lang/cargo/blob/master/src/cargo/util/toml/mod.rs — how cargo modifies user files carefully

  **WHY Each Reference Matters**:
  - cargo user-file-modification: conservative pattern (read, check, only modify if needed, preserve line endings)

  **Acceptance Criteria**:
  - [ ] Repo sem `.gitignore` → cria com `.martins/\n`, retorna `Created`
  - [ ] Repo com `.gitignore` contendo `.martins/` → retorna `NoChange`, não modifica
  - [ ] Repo com `.gitignore` sem `.martins/` → append `.martins/\n`, retorna `Appended`
  - [ ] Repo com `.gitignore` terminando sem newline → append `\n.martins/\n` (preserva formatting)

  **QA Scenarios**:

  ```
  Scenario: Creates .gitignore when absent
    Tool: Bash
    Preconditions: T1+T14 complete
    Steps:
      1. cargo test config::tests::ensure_gitignore_create -- --nocapture
    Expected Result: TempDir with no .gitignore → ensure_gitignore returns Created. File exists, content is ".martins/\n".
    Evidence: .sisyphus/evidence/task-14-create.log

  Scenario: No-op when already present
    Tool: Bash
    Preconditions: T1+T14 complete
    Steps:
      1. cargo test config::tests::ensure_gitignore_noop -- --nocapture
    Expected Result: TempDir with .gitignore containing ".martins/" → returns NoChange. File byte-identical before and after.
    Evidence: .sisyphus/evidence/task-14-noop.log

  Scenario: Appends to existing .gitignore
    Tool: Bash
    Preconditions: T1+T14 complete
    Steps:
      1. cargo test config::tests::ensure_gitignore_append -- --nocapture
    Expected Result: .gitignore with existing "target/" → returns Appended. File now ends with "target/\n.martins/\n".
    Evidence: .sisyphus/evidence/task-14-append.log
  ```

  **Commit**: YES — `feat(bootstrap): auto-gitignore .martins/ on first run`

### Wave 3 — UI panes + terminal widget (T15-T21, paralelizáveis)

> **📐 DESIGN CONTRACT**: Todas as tasks de UI (T15-T21) DEVEM consultar `.sisyphus/plans/UI-SPEC.md` como fonte única de verdade visual. O UI-SPEC contém:
> - Design tokens (palette hex + ratatui Color constants)
> - Typography rules + iconography lexicon
> - 30 screen states (20 mockups em Paper canvas + 10 text-only specs)
> - 5 flow diagrams (Mermaid) cobrindo first-run, create workspace, mode transitions, shutdown, preview/edit
> - Component specs (C1-C8): sidebar_left, sidebar_right, terminal pane, status bar, modal, preview, fuzzy picker
> - Mode indicator contract (Normal vs Terminal — não-negociável)
> - Responsive breakpoints
>
> Se algum estado não estiver no UI-SPEC, PARE e levante a questão — não improvise.

- [x] 15. **Responsive 3-pane layout (collapse sidebars ≤120/≤100/<80)**

  **What to do**:
  - Em `src/ui/layout.rs`:
    - `pub struct LayoutState { show_left: bool, show_right: bool }` persistido via user toggles
    - `pub fn compute(frame_size: Rect, state: &LayoutState) -> PaneRects { left: Option<Rect>, terminal: Rect, right: Option<Rect>, status_bar: Rect }`:
      - Frame width < 80: retorna erro → render "resize to 80×24" overlay
      - Frame width 80-99: `show_left = false`, `show_right = false` forced
      - Frame width 100-119: `show_right = false` forced (left respeita user toggle)
      - Frame width ≥120: respeita user toggles
    - Sidebars largura: `min(30, max(20, 20% of frame))` cada
    - Terminal pane: espaço restante
    - Status bar: 1 linha no bottom
  - Em `src/ui/mod.rs`: `pub fn render_root(frame: &mut Frame, app: &App)`:
    - Computa PaneRects
    - Renderiza cada pane (delega para sidebar_left, terminal, sidebar_right modules)
    - Renderiza status bar com `[NORMAL]` / `[TERMINAL]` + repo atual + workspace ativo

  **Must NOT do**:
  - NÃO hardcodar widths (tudo derivado do frame)
  - NÃO usar floats para sizing (tudo u16 / Rect)
  - NÃO renderizar antes de validar frame ≥ 80×24

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering` — Layout responsivo é engenharia visual
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T16-T21
  - **Parallel Group**: Wave 3
  - **Blocks**: T16, T17, T18
  - **Blocked By**: T8

  **References**:
  - ratatui Layout: https://docs.rs/ratatui/latest/ratatui/layout/index.html
  - Pattern: https://github.com/ratatui-org/templates/blob/main/async/src/ui.rs — responsive layout pattern
  - Pattern: https://github.com/helix-editor/helix/blob/master/helix-term/src/ui/editor.rs — sidebar collapse in Helix

  **WHY Each Reference Matters**:
  - ratatui Layout API: `Layout::default().direction().constraints()` é canônico
  - helix: pattern de sidebar collapsing baseado em width, exato caso de uso

  **Acceptance Criteria**:
  - [ ] Frame 200×60: 3 panes visíveis (left 30, terminal middle, right 30, status 1)
  - [ ] Frame 110×40: left + terminal + status (right forced hidden)
  - [ ] Frame 85×30: terminal + status (both sidebars hidden)
  - [ ] Frame 70×24: error overlay "resize to 80×24"
  - [ ] User toggle right sidebar (Ctrl+N) em 200×60 → hidden; toggle again → shown

  **QA Scenarios**:

  ```
  Scenario: Snapshot all 4 responsive breakpoints
    Tool: Bash
    Preconditions: T1+T8+T15 complete. Golden snapshots committed at src/ui/snapshots/
    Steps:
      1. INSTA_UPDATE=no cargo test ui::layout::tests::responsive_snapshots -- --nocapture
    Expected Result: 4 snapshot assertions pass deterministically against committed *.snap files (200x60, 110x40, 85x30, 70x24). Zero pending/new snapshots. Exit 0.
    Evidence: .sisyphus/evidence/task-15-snapshots.log

  Scenario: User toggle persists
    Tool: Bash
    Preconditions: T1+T8+T15 complete
    Steps:
      1. cargo test ui::layout::tests::user_toggle -- --nocapture
    Expected Result: Initially right visible at width 200. Toggle to hidden. Next render respects hidden. Toggle back. Visible.
    Evidence: .sisyphus/evidence/task-15-toggle.log
  ```

  **Commit**: YES — `feat(ui): responsive 3-pane layout with collapse thresholds`

- [x] 16. **Sidebar left: workspaces tree + archived section (single-repo MVP)**

  **What to do**:
  - Em `src/ui/sidebar_left.rs`:
    - `pub fn render(frame: &mut Frame, area: Rect, app: &App, focused: bool)`:
      - Border: nome do repo (basename de `active_repo`) como título (ex: "myproject")
      - Layout:
        ```
        Workspaces
          ● caetano          [active, tab 2/3]
          ○ gil              [inactive]
          ◐ elis             [exited 42]
        ▼ Archived (3)
          caetano-2
          chico
          joao-gilberto
        ```
      - Status icons: `●` active (running PTY), `○` inactive (status: Inactive), `◐` exited (status: Exited), `⋯` archived
      - Selecionado: highlighted com `Style::reversed()`
      - Se focado: border `Style::bold().fg(Yellow)`; senão `Style::dim()`
      - Suporte scroll se lista > área disponível (`ratatui::widgets::List` com `state`)

  **Must NOT do**:
  - NÃO mostrar paths absolutos (só nomes)
  - NÃO mostrar agent-specific info na sidebar (apenas status)
  - NÃO implementar drag-to-reorder

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T15, T17-T21
  - **Parallel Group**: Wave 3
  - **Blocks**: T22
  - **Blocked By**: T10, T15

  **References**:
  - ratatui List widget: https://docs.rs/ratatui/latest/ratatui/widgets/struct.List.html
  - Pattern: https://github.com/extrawurst/gitui/blob/master/src/components/commitlist.rs — scrollable list with selection
  - Pattern: https://github.com/ratatui-org/templates/blob/main/async/src/tui/components/ — sidebar in template

  **WHY Each Reference Matters**:
  - gitui commitlist: exact pattern for scrollable selectable list
  - ratatui List: API canônica (com `ListState` para selection)

  **Acceptance Criteria**:
  - [ ] Snapshot: 1 repo, 3 workspaces (1 active, 1 inactive, 1 exited), Archived section com 3
  - [ ] Navigation: j/k move selection, Enter selects, u toggles archive
  - [ ] Focus styling: border color muda com `focused` bool
  - [ ] Scroll: 20 workspaces em área de 10 linhas → scrollbar e scroll funciona
  - [ ] Empty state (repo sem workspaces): "No workspaces. Press 'n' to create one."

  **QA Scenarios**:

  ```
  Scenario: Full sidebar snapshot with all states
    Tool: Bash
    Preconditions: T1+T10+T15+T16 complete. Golden snapshot committed.
    Steps:
      1. INSTA_UPDATE=no cargo test ui::sidebar_left::tests::full_snapshot -- --nocapture
    Expected Result: Snapshot matches committed golden deterministically. Active shows ●, inactive ○, exited ◐, archived in collapsed section. Zero pending snapshots.
    Evidence: .sisyphus/evidence/task-16-snapshot.log

  Scenario: Empty state when no workspaces
    Tool: Bash
    Preconditions: T1+T16 complete. Golden snapshot committed.
    Steps:
      1. INSTA_UPDATE=no cargo test ui::sidebar_left::tests::empty_state -- --nocapture
    Expected Result: Deterministic match. Renders repo name header + message "No workspaces. Press 'n' to create one." centered.
    Evidence: .sisyphus/evidence/task-16-empty.log

  Scenario: Scroll with 20 workspaces in 10-line area
    Tool: Bash
    Preconditions: T1+T16 complete
    Steps:
      1. cargo test ui::sidebar_left::tests::scroll -- --nocapture
    Expected Result: Initial render shows items 1-10. After 5x DOWN key, selection is on item 6. After 10x DOWN, selection is on item 11 and viewport scrolled.
    Evidence: .sisyphus/evidence/task-16-scroll.log
  ```

  **Commit**: YES — `feat(ui): left sidebar with workspace tree and archived section`

- [x] 17. **Sidebar right: modified files with status icons**

  **What to do**:
  - Em `src/ui/sidebar_right.rs`:
    - `pub fn render(frame: &mut Frame, area: Rect, files: &[FileEntry], focused: bool)`:
      - Border: "Changes (N)" title
      - Cada entry: ícone + path relativo ao worktree
      - Ícones: `M` yellow, `A` green, `D` red, `R` blue, `?` gray (untracked)
      - Truncar paths longos: `...foo/bar/baz.rs` (prefix com `...` se > área width-3)
      - Ordenar: untracked primeiro, depois A/M/D/R por path
      - Scrollable com ListState
      - Focused: border bold yellow; senão dim
      - Integração com watcher: debouncer dispara refresh do `files` via channel para App state
  - App state tem `files: Vec<FileEntry>` por workspace ativo
  - Quando workspace muda, trigger async `git::diff::modified_files`

  **Must NOT do**:
  - NÃO mostrar diff inline (só lista)
  - NÃO permitir staging/commit dali
  - NÃO re-executar diff em cada frame (só quando watcher dispara ou workspace muda)

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T15, T16, T18-T21
  - **Parallel Group**: Wave 3
  - **Blocks**: T21, T22
  - **Blocked By**: T13, T15

  **References**:
  - ratatui List + ListState: https://docs.rs/ratatui/latest/ratatui/widgets/struct.List.html
  - Pattern: https://github.com/extrawurst/gitui/blob/master/src/components/status_tree.rs — exact same UI element

  **WHY Each Reference Matters**:
  - gitui status_tree: literalmente o mesmo componente (status icons + path list)

  **Acceptance Criteria**:
  - [ ] Snapshot: 5 arquivos com status variados, truncation em path longo
  - [ ] Empty state: "No changes." centered
  - [ ] Refresh: trigger via channel → re-run diff → atualiza list
  - [ ] Ordering: untracked primeiro, depois alphabetical
  - [ ] Selection com j/k; Enter dispara preview (T21)

  **QA Scenarios**:

  ```
  Scenario: Snapshot with all status types
    Tool: Bash
    Preconditions: T1+T13+T15+T17 complete. Golden snapshot committed.
    Steps:
      1. INSTA_UPDATE=no cargo test ui::sidebar_right::tests::all_statuses -- --nocapture
    Expected Result: Deterministic match. 5 files render with correct icons: untracked first (?), then A/M/D/R alphabetical. Colors correct per status. Zero pending snapshots.
    Evidence: .sisyphus/evidence/task-17-statuses.log

  Scenario: Long path truncation
    Tool: Bash
    Preconditions: T1+T17 complete (width 30)
    Steps:
      1. cargo test ui::sidebar_right::tests::long_path -- --nocapture
    Expected Result: path "very/deeply/nested/dir/file.rs" in 30-width pane renders as "M ...dir/file.rs"
    Evidence: .sisyphus/evidence/task-17-truncate.txt

  Scenario: Refresh triggered by watcher event
    Tool: Bash
    Preconditions: T1+T12+T13+T17 complete
    Steps:
      1. cargo test ui::sidebar_right::tests::watcher_refresh -- --nocapture
    Expected Result: Initial render shows 0 files. Simulate watcher event. Wait for diff re-run. Next render shows new file count.
    Evidence: .sisyphus/evidence/task-17-refresh.log
  ```

  **Commit**: YES — `feat(ui): right sidebar with modified files list`

- [x] 18. **Terminal pane: tui-term PseudoTerminal wrapper + tabs**

  **What to do**:
  - Em `src/ui/terminal.rs`:
    - `pub fn render(frame: &mut Frame, area: Rect, ws: &Workspace, active_tab: usize, mode: InputMode, focused: bool)`:
      - Área dividida: 1 linha tab bar no top + resto para terminal
      - Tab bar: `[1*] [2] [3]` com `*` marcando active (highlighted)
      - Terminal: `tui_term::widget::PseudoTerminal::new(&session.parser.read().screen())`
      - Border:
        - focused + Normal: yellow solid
        - focused + Terminal: green solid (indicação visual clara)
        - unfocused: gray dim
      - Quando mode == Terminal: cursor blinking real (vt100 parser já trata)
  - `src/pty/manager.rs`:
    - `pub struct PtyManager { sessions: HashMap<(WorkspaceId, TabId), PtySession> }`
    - `pub fn spawn_tab(&mut self, ws_id, tab_id, cwd, agent) -> Result<()>`
    - `pub fn write_input(&self, ws_id, tab_id, bytes: &[u8]) -> Result<()>`
    - `pub fn resize_all_for(&self, ws_id, rows, cols) -> Result<()>`: resize todas as tabs do workspace (elas têm sizes iguais)
    - `pub fn close_tab(&mut self, ws_id, tab_id) -> Result<()>`: kill + remove
    - `pub fn close_workspace(&mut self, ws_id) -> Result<()>`: close all tabs

  **Must NOT do**:
  - NÃO criar tab bar renomeável (só números 1-5)
  - NÃO permitir splits (só tabs)
  - NÃO exceder 5 tabs por workspace (enforce no manager)
  - NÃO renderizar quando parser está sendo escrito (usar `try_read()`; skip frame se contended)

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain` — Sincronização de render com PTY read, Arc<RwLock> timing, tab state
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T15, T16, T17, T19-T21
  - **Parallel Group**: Wave 3
  - **Blocks**: T22
  - **Blocked By**: T8, T11, T15

  **References**:
  - tui-term basic example: https://github.com/a-kenji/tui-term/blob/main/examples/simple_ls_chan.rs
  - tui-term nested_shell_async: https://github.com/a-kenji/tui-term/blob/main/examples/nested_shell_async.rs
  - Turborepo pane: https://github.com/vercel/turborepo/blob/main/crates/turborepo-ui/src/tui/pane.rs
  - Pattern: https://docs.rs/tui-term/latest/tui_term/widget/struct.PseudoTerminal.html

  **WHY Each Reference Matters**:
  - tui-term examples são a fonte oficial de como integrar parser → widget
  - turborepo pane.rs é production-grade com tab bar + PTY manager

  **Acceptance Criteria**:
  - [ ] 1 workspace com bash → parser recebe output → render mostra no pane
  - [ ] Criar 3 tabs → tab bar mostra `[1] [2*] [3]`
  - [ ] Switch tab (1/2/3 keys em modo Normal) → muda active parser exibido
  - [ ] Max 5 tabs: 6ª tentativa → `Err(TabLimit)`
  - [ ] Close tab (T key) → mata PTY, remove do manager, ajusta indices
  - [ ] Border color muda com mode (Normal amarelo, Terminal verde)
  - [ ] `try_read()` no render: se parser está sendo escrito → skip frame (no panic)

  **QA Scenarios**:

  ```
  Scenario: Embedded terminal renders bash output
    Tool: Bash
    Preconditions: T1+T8+T11+T15+T18 complete. Golden snapshot committed (expects deterministic bash prompt; test uses `PS1=$` env to normalize prompt).
    Steps:
      1. INSTA_UPDATE=no cargo test ui::terminal::tests::render_bash -- --nocapture
    Expected Result: Deterministic match. Spawn bash, write "echo TESTX\n", wait 200ms. Screen contains "TESTX". Zero pending snapshots.
    Evidence: .sisyphus/evidence/task-18-bash.log

  Scenario: Tab bar and switching
    Tool: Bash
    Preconditions: T1+T11+T18 complete
    Steps:
      1. cargo test ui::terminal::tests::tab_switching -- --nocapture
    Expected Result: 3 tabs with unique PIDs. Tab bar rendered as "[1] [2*] [3]". Switch to 1 via key '1', tab bar becomes "[1*] [2] [3]". Parser visible is tab 1's.
    Evidence: .sisyphus/evidence/task-18-tabs.txt

  Scenario: Max 5 tabs enforcement
    Tool: Bash
    Preconditions: T1+T11+T18 complete
    Steps:
      1. cargo test ui::terminal::tests::tab_limit -- --nocapture
    Expected Result: spawn_tab called 5 times succeeds. 6th call returns Err(TabLimit).
    Evidence: .sisyphus/evidence/task-18-limit.log

  Scenario: Border reflects input mode
    Tool: Bash
    Preconditions: T1+T8+T18 complete. Golden snapshots committed.
    Steps:
      1. INSTA_UPDATE=no cargo test ui::terminal::tests::mode_border -- --nocapture
    Expected Result: 2 deterministic snapshot assertions pass: @NormalMode (yellow border), @TerminalMode (green border). Clear visual distinction. Zero pending snapshots.
    Evidence: .sisyphus/evidence/task-18-mode-border.log
  ```

  **Commit**: YES — `feat(ui): embedded terminal pane with tabs and mode indicator`

- [x] 19. **Modal system: new workspace, confirm delete, install binaries**

  **What to do**:
  - Em `src/ui/modal.rs`:
    - `pub enum Modal { None, NewWorkspace(NewWorkspaceForm), ConfirmDelete(DeleteForm), InstallMissing(InstallForm) }`
    - `pub struct NewWorkspaceForm { name_input: String, auto_generate: bool, agent: Agent, base_branch: String, branches_available: Vec<String> }`
    - `pub struct DeleteForm { workspace_name: String, unpushed_commits: usize, delete_branch: bool }`
    - `pub struct InstallForm { missing_tools: Vec<Tool>, confirmed: bool }`
    - `pub fn render(frame: &mut Frame, modal: &Modal)`: centered popup com `Clear` widget + bordered block
    - NewWorkspace modal:
      ```
      ┌─ New Workspace ─────────────────┐
      │ Name: [caetano___________] [R]  │  (R = regenerate random)
      │ Base branch: main ▼             │
      │ Agent: opencode ▼               │
      │                                 │
      │ [Enter] Create  [Esc] Cancel    │
      └─────────────────────────────────┘
      ```
    - ConfirmDelete modal quando unpushed > 0: warning em vermelho "⚠ N unpushed commits lost"
    - InstallMissing modal: lista missing tools + comandos propostos + "[y] install / [n] skip"

  **Must NOT do**:
  - NÃO implementar campos livres além dos listados
  - NÃO fazer input validation complexa (usar `mpb::validate` para nome)
  - NÃO permitir install com flags custom (só defaults)

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering` — Modals são UI pura
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T15-T18, T20, T21
  - **Parallel Group**: Wave 3
  - **Blocks**: T22
  - **Blocked By**: T3, T6, T10

  **References**:
  - ratatui Clear widget: https://docs.rs/ratatui/latest/ratatui/widgets/struct.Clear.html
  - Pattern: https://github.com/ratatui-org/ratatui/blob/main/examples/popup.rs — centered popup pattern
  - Pattern: https://github.com/extrawurst/gitui/blob/master/src/popups/create_branch.rs — exact same use case

  **WHY Each Reference Matters**:
  - ratatui popup example: idiom exato de centralized modal com Clear
  - gitui create_branch: pattern completo de form modal com input + dropdowns

  **Acceptance Criteria**:
  - [ ] Snapshot cada modal em estado default
  - [ ] NewWorkspace: name editing, `R` regenera nome MPB, dropdown base branch navegável
  - [ ] ConfirmDelete sem unpushed: sem warning; com 5 unpushed: warning vermelho "⚠ 5 unpushed commits"
  - [ ] InstallMissing: lista tools + Y installa (spawn cmd), N/Esc cancela
  - [ ] Esc fecha modal sem aplicar

  **QA Scenarios**:

  ```
  Scenario: NewWorkspace modal with auto-name
    Tool: Bash
    Preconditions: T1+T3+T19 complete
    Preconditions: T1+T3+T19 complete. Golden snapshots committed (uses seeded PRNG for deterministic MPB name).
    Steps:
      1. INSTA_UPDATE=no cargo test ui::modal::tests::new_workspace_auto -- --nocapture
    Expected Result: 2 deterministic snapshot assertions pass. Modal renders centered. Pressing 'R' regenerates name field to another MPB artist (seeded). Zero pending snapshots.
    Evidence: .sisyphus/evidence/task-19-new.log

  Scenario: Delete with unpushed warning
    Tool: Bash
    Preconditions: T1+T10+T19 complete. Golden snapshot committed.
    Steps:
      1. INSTA_UPDATE=no cargo test ui::modal::tests::delete_with_unpushed -- --nocapture
    Expected Result: Deterministic match. Modal shows "⚠ WARNING: 5 unpushed commits on this branch will be permanently lost." in red. Checkbox for delete branch. Zero pending snapshots.
    Evidence: .sisyphus/evidence/task-19-delete.log

  Scenario: Install modal shows correct commands per OS
    Tool: Bash
    Preconditions: T1+T6+T19 complete
    Steps:
      1. cargo test ui::modal::tests::install_macos -- --nocapture
    Expected Result: On macOS mocked env, modal shows "brew install bat" for Bat. Y/N selectable.
    Evidence: .sisyphus/evidence/task-19-install.txt
  ```

  **Commit**: YES — `feat(ui): modals for create delete and install-missing`

- [x] 20. **Fuzzy picker: workspaces + modified files**

  **What to do**:
  - Em `src/ui/picker.rs` (usa `nucleo-matcher`, já em deps de T1):
    - `pub enum PickerKind { Workspaces, ModifiedFiles }`
    - `pub struct Picker { input: String, items: Vec<String>, filtered: Vec<(usize, i32)>, kind: PickerKind, selected: usize }`
    - `pub fn render(frame: &mut Frame, picker: &Picker)`: overlay centered, 60% width × 50% height
      - Top: input line com prompt `> `
      - Middle: filtered list (top 20 matches por score)
      - Bottom: N/M match count
    - `pub fn update_filter(&mut self)`: re-ranqueia com nucleo-matcher
    - `pub fn on_key(&mut self, key: KeyEvent) -> PickerOutcome`:
      - char → append ao input, re-filter
      - Backspace → pop
      - j/k (ou Down/Up, fora de input) → navigate
      - Enter → return `Selected(index)`
      - Esc → return `Cancelled`

  **Must NOT do**:
  - NÃO buscar conteúdo de arquivos (apenas nomes/paths)
  - NÃO suportar regex (apenas fuzzy)
  - NÃO implementar history (simple)

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T15-T19, T21
  - **Parallel Group**: Wave 3
  - **Blocks**: T22
  - **Blocked By**: T10, T13

  **References**:
  - nucleo-matcher docs: https://docs.rs/nucleo-matcher/latest/
  - Pattern: https://github.com/helix-editor/helix/blob/master/helix-term/src/ui/picker.rs — production picker
  - Pattern: https://github.com/zellij-org/zellij/blob/main/default-plugins/strider/src/main_view.rs — zellij file picker

  **WHY Each Reference Matters**:
  - helix picker: pattern canônico de picker em Rust TUI (input + filter + list)
  - nucleo-matcher: mesma biblioteca que helix/zed usam (qualidade garantida)

  **Acceptance Criteria**:
  - [ ] Type "cae" → lista filtrada com "caetano" no topo
  - [ ] Snapshot picker vazio, com input, com 20 matches
  - [ ] Enter → `Selected(index)` → app route para o item
  - [ ] Esc → Cancelled → picker close
  - [ ] 1000 items → filter em <50ms (nucleo é fast)

  **QA Scenarios**:

  ```
  Scenario: Fuzzy filter narrows results
    Tool: Bash
    Preconditions: T1+T20 complete
    Steps:
      1. cargo test ui::picker::tests::fuzzy_filter -- --nocapture
    Expected Result: 10 items. Input "cae" → 1-3 matches with "caetano" first. Input "xyz" → 0 matches.
    Evidence: .sisyphus/evidence/task-20-filter.log

  Scenario: Navigation and selection
    Tool: Bash
    Preconditions: T1+T20 complete
    Steps:
      1. cargo test ui::picker::tests::navigate_select -- --nocapture
    Expected Result: 5 filtered. Down key 3 times → selected=3. Enter → returns Selected(3). Esc → returns Cancelled.
    Evidence: .sisyphus/evidence/task-20-nav.log

  Scenario: Performance with 1000 items
    Tool: Bash
    Preconditions: T1+T20 complete
    Steps:
      1. cargo test ui::picker::tests::perf_1000 --release -- --nocapture
    Expected Result: 1000 workspace names. Type query "abc". Filter completes in <50ms measured via Instant::now() diff.
    Evidence: .sisyphus/evidence/task-20-perf.log
  ```

  **Commit**: YES — `feat(ui): fuzzy picker for workspaces and files`

- [x] 21. **Bat preview overlay + $EDITOR spawn**

  **What to do**:
  - Em `src/ui/preview.rs`:
    - `pub struct PreviewOverlay { file_path: PathBuf, session: PtySession }`
    - `pub fn open(file_path: PathBuf, area: Rect) -> Result<PreviewOverlay>`:
      - Spawn PTY temporário com comando: `bat --paging=always --color=always {file_path}`
      - Se bat não instalado → fallback `less {file_path}` OU `cat {file_path}` em overlay
      - Area: 90% do frame, centered
    - `pub fn render(frame: &mut Frame, overlay: &PreviewOverlay, area: Rect)`: usa PseudoTerminal widget do tui-term
    - `pub fn on_key(&mut self, key: KeyEvent) -> PreviewOutcome`:
      - q ou Esc → Close (kill PTY)
      - outras → forward pro PTY (bat/less respeita j/k/G/gg/etc)
  - Em `src/editor.rs`:
    - `pub async fn open_in_editor(file_path: &Path) -> Result<()>`:
      - Lê `EDITOR` env, fallback `VISUAL`, fallback `nvim`/`vim`/`vi`/`nano`
      - `crossterm::terminal::disable_raw_mode`
      - `crossterm::terminal::LeaveAlternateScreen`
      - Spawn editor em foreground (herda stdin/stdout/stderr), await exit
      - `crossterm::terminal::EnterAlternateScreen`
      - `crossterm::terminal::enable_raw_mode`
      - Force redraw flag → App sabe que precisa re-render

  **Must NOT do**:
  - NÃO renderizar preview com syntect inline (apenas via bat em PTY overlay)
  - NÃO filtrar input do bat (passar tudo)
  - NÃO tentar detectar editor modal (nvim/vim vs nano são iguais para nós)

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high` — Editor handoff é delicado (terminal modes)
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T15-T20
  - **Parallel Group**: Wave 3
  - **Blocks**: T22
  - **Blocked By**: T17

  **References**:
  - bat CLI options: https://github.com/sharkdp/bat#command-line-options
  - crossterm terminal modes: https://docs.rs/crossterm/latest/crossterm/terminal/index.html
  - Pattern: https://github.com/extrawurst/gitui/blob/master/src/input.rs — editor handoff
  - Pattern: https://docs.rs/edit/latest/edit/ — simple crate for $EDITOR handoff (reference, não usar direto)

  **WHY Each Reference Matters**:
  - gitui input.rs: production pattern de suspend→editor→resume
  - bat options: sabemos exatamente quais flags passar (paging always + color always)

  **Acceptance Criteria**:
  - [ ] Preview em file → PTY overlay com bat renderizado com syntax highlighting
  - [ ] q fecha overlay, kill PTY
  - [ ] Se bat não instalado → overlay roda `less`, ainda navegável
  - [ ] `open_in_editor` com $EDITOR=nvim → spawn nvim, esperar exit, restore TUI
  - [ ] Modificações no editor → watcher triggera refresh da sidebar direita

  **QA Scenarios**:

  ```
  Scenario: Bat preview overlay renders content
    Tool: Bash
    Preconditions: T1+T21 complete. bat installed.
    Steps:
      1. cargo test ui::preview::tests::bat_overlay -- --nocapture
    Expected Result: TempFile with "fn main() {}\n". Open preview. Wait 500ms. Parser contents contain "fn main". q closes, PTY killed.
    Evidence: .sisyphus/evidence/task-21-bat.log

  Scenario: Editor spawn and restore
    Tool: Bash (tmux)
    Preconditions: T1+T21+T22 complete. nvim installed. Invoked from project root. Test repo setup inline (no pre-configured git identity assumed).
    Steps:
      1. PROJECT_ROOT="$(git rev-parse --show-toplevel)" && EVIDENCE="$PROJECT_ROOT/.sisyphus/evidence" && mkdir -p "$EVIDENCE"
      2. BIN="$PROJECT_ROOT/target/release/martins"
      3. rm -rf /tmp/martins-e-test && mkdir -p /tmp/martins-e-test && cd /tmp/martins-e-test
      4. git init -q && git -c user.name=test -c user.email=test@example.com commit -q --allow-empty -m init
      5. echo "initial" > README.md && git add -A && git -c user.name=test -c user.email=test@example.com commit -q -m "add readme"
      6. tmux kill-session -t ed 2>/dev/null; tmux new-session -d -s ed -x 150 -y 40 "cd /tmp/martins-e-test && EDITOR=nvim $BIN"
      7. sleep 2
      8. # Create workspace
      9. tmux send-keys -t ed 'N' '' ; sleep 1; tmux send-keys -t ed Enter ''; sleep 3
      10. # Find worktree path (martins creates sibling dir "martins-e-test-<mpb>")
      11. WORKTREE=$(ls -td /tmp/martins-e-test-* 2>/dev/null | head -1); test -n "$WORKTREE" || { echo "no worktree found"; exit 1; }
      12. # Modify a file in worktree EXTERNALLY so it shows up in sidebar right
      13. echo "changed by test" >> "$WORKTREE/README.md"
      14. # Wait for watcher debounce (750ms) + UI refresh
      15. sleep 2
      16. tmux capture-pane -p -t ed > "$EVIDENCE/task-21-editor-01-before.txt"
      17. # Confirm "README.md" appears in right sidebar (Changes section)
      18. grep -q "README" "$EVIDENCE/task-21-editor-01-before.txt" || { echo "README.md missing from changes sidebar"; exit 1; }
      19. # Focus right sidebar (Tab twice: Left→Terminal→Right)
      20. tmux send-keys -t ed Tab ''; sleep 0.3; tmux send-keys -t ed Tab ''; sleep 0.3
      21. # Select first (only) file
      22. tmux send-keys -t ed 'j' ''; sleep 0.3
      23. # Open in $EDITOR
      24. tmux send-keys -t ed 'e' ''; sleep 2
      25. tmux capture-pane -p -t ed > "$EVIDENCE/task-21-editor-02-nvim.txt"
      26. # Quit nvim
      27. tmux send-keys -t ed Escape ''; sleep 0.3; tmux send-keys -t ed ':q' Enter ''; sleep 2
      28. tmux capture-pane -p -t ed > "$EVIDENCE/task-21-editor-03-after.txt"
      29. tmux send-keys -t ed C-q ''; sleep 2
      30. tmux kill-session -t ed 2>/dev/null
    Expected Result: File 01-before contains "README" in sidebar right. File 02-nvim contains tilde markers (`~`) at line starts (nvim empty-buffer lines) OR shows README.md content. File 03-after shows martins TUI layout restored (contains "Workspaces" header, not nvim markers). Step 18 passes (file was detected).
    Evidence: $PROJECT_ROOT/.sisyphus/evidence/task-21-editor-*.txt (3 captures)

  Scenario: Fallback when bat missing
    Tool: Bash
    Preconditions: T1+T21 complete
    Steps:
      1. # Test internally scopes PATH lookup (e.g. via `which::which_in(custom_path)`) — ambient cargo PATH is preserved.
      2. cargo test ui::preview::tests::fallback_less -- --nocapture
    Expected Result: Test injects a PATH without bat (e.g. only /tmp) into the preview module's lookup. Preview spawns less as fallback. Test asserts child process command contains "less".
    Evidence: .sisyphus/evidence/task-21-fallback.log
  ```

  **Commit**: YES — `feat(ui): bat preview overlay and editor spawn`

### Wave 4 — Integration + Polish + Distribution (T22-T26)

- [x] 22. **Main event loop: async multiplex + graceful shutdown**

  **What to do**:
  - Em `src/app.rs`:
    - `pub struct App { state: AppState, active_repo: PathBuf, active_workspace: Option<String>, active_tab: usize, mode: InputMode, focus: Pane, layout: LayoutState, modal: Modal, picker: Option<Picker>, preview: Option<PreviewOverlay>, pty_mgr: PtyManager, watcher: Watcher, files: HashMap<String, Vec<FileEntry>>, should_quit: bool }`
    - `pub enum Pane { Left, Terminal, Right }`
  - Em `src/main.rs` (expand):
    - `#[tokio::main] async fn main() -> Result<()>`:
      1. Parse `--version`/`-V`/`--help`/`-h` flags. If matched → print and exit 0 (BEFORE any TUI init).
      2. `tracing` init (T7) — file-only logging so nothing corrupts TUI later.
      3. `Repository::discover(cwd)` → repo_root. If not in a git repo → eprint "martins must be run from inside a git repository" and exit 2 (BEFORE any TUI init).
      4. `ensure_gitignore(repo_root)` (T14) — non-TUI file op.
      5. `AppState::load(repo_root)` (T4) — non-TUI file op.
      6. `preflight_tools()` (T6) — pure detection, returns `MissingTools`. NO UI rendered here. Result stored in a local variable.
      7. Install panic hook that restores terminal BEFORE unwinding (T7).
      8. Setup terminal: enable_raw_mode, EnterAlternateScreen, hide_cursor, create `Terminal` backend (TUI now initialized).
      9. If `MissingTools` from step 6 is non-empty → set `App.modal = Modal::InstallMissing(...)` so the first `terminal.draw(...)` in the main loop renders the modal as the initial UI state. (Modal is rendered by the already-initialized TUI, not before.)
      10. Event stream: `crossterm::event::EventStream::new()` + tick timer 16ms
      11. File watcher events channel
      12. Main loop:
          ```rust
          loop {
            if shutdown_requested { break }
            terminal.draw(|f| ui::render_root(f, &app))?;
            tokio::select! {
              event = events.next() => app.handle_crossterm(event),
              fs = watcher.next_event() => app.handle_fs_event(fs),
              pty_exit = pty_mgr.next_exit() => app.handle_pty_exit(pty_exit),
              _ = tick.tick() => {} // redraw only
            }
          }
          ```
    - Graceful shutdown:
      1. Set `should_quit = true`
      2. Para cada PtySession ativa: `child.kill()` (SIGHUP)
      3. `tokio::time::sleep(3s)` com `select!` por exits
      4. Survivors: SIGKILL (não há API direta em portable-pty, mas child.kill() já é best-effort)
      5. Join dos spawn_blocking tasks (veem EOF)
      6. `AppState::save(repo_root)` — marca todos workspaces ativos como `Inactive`
      7. `disable_raw_mode` + `LeaveAlternateScreen` + `show_cursor`
  - Handle cada action: dispatches para operações git/pty/state

  **Must NOT do**:
  - NÃO chamar git2 no main thread (sempre spawn_blocking)
  - NÃO bloquear o event loop por mais que 1 tick (16ms)
  - NÃO esquecer de restaurar terminal em qualquer caminho (incluindo panic/error)

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain` — Multiplex async complexo, shutdown correctness crítico, edge cases de sync/async boundary
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: NO — depende de todos os módulos
  - **Parallel Group**: Wave 4
  - **Blocks**: T24, F1-F4
  - **Blocked By**: T4, T10, T11, T14, T16, T17, T18, T19, T20, T21

  **References**:
  - ratatui async template: https://github.com/ratatui-org/templates/tree/main/async
  - Pattern: https://github.com/vercel/turborepo/blob/main/crates/turborepo-ui/src/tui/app.rs — production main loop
  - Pattern: https://github.com/extrawurst/gitui/blob/master/src/app.rs — App struct pattern

  **WHY Each Reference Matters**:
  - ratatui async template: referência oficial para multiplex com tokio::select
  - turborepo app.rs: production-grade shutdown sequence + tab management
  - gitui app.rs: como estruturar App struct com múltiplos componentes

  **Acceptance Criteria**:
  - [ ] `martins --version` imprime versão e sai (0)
  - [ ] `martins` fora de um repo git → imprime erro "must be run from inside a git repository" e sai com código 2
  - [ ] `martins` dentro de um repo git → abre TUI com sidebar esquerda mostrando nome do repo + seção "Workspaces" vazia
  - [ ] Criar workspace → spawn opencode → terminal visível
  - [ ] Ctrl+Q → graceful shutdown, state salvo, terminal restaurado em <5s
  - [ ] Panic durante render → terminal restaurado (sem raw mode lock)
  - [ ] 3 workspaces ativos + arquivar 1 → state.json tem 3 (2 active, 1 archived)
  - [ ] Frame rate ≤ 60 FPS mesmo com 5 PTYs ativas

  **QA Scenarios**:

  ```
  Scenario: Start, create workspace, shutdown — full loop
    Tool: Bash (tmux)
    Preconditions: All prior tasks complete. Invoked from martins project root.
    Steps:
      1. PROJECT_ROOT="$(git rev-parse --show-toplevel)" && EVIDENCE="$PROJECT_ROOT/.sisyphus/evidence" && mkdir -p "$EVIDENCE"
      2. BIN="$PROJECT_ROOT/target/release/martins"
      3. rm -rf /tmp/test-repo && mkdir -p /tmp/test-repo && cd /tmp/test-repo && git init -q && git -c user.name=test -c user.email=test@example.com commit -q --allow-empty -m init
      4. tmux kill-session -t m 2>/dev/null; tmux new-session -d -s m -x 150 -y 40 "cd /tmp/test-repo && $BIN"
      5. sleep 2; tmux capture-pane -p -t m > "$EVIDENCE/task-22-01-start.txt"
      6. tmux send-keys -t m 'n' ''; sleep 1
      7. tmux capture-pane -p -t m > "$EVIDENCE/task-22-02-modal.txt"
      8. tmux send-keys -t m 'N' ''; sleep 0.5
      9. tmux capture-pane -p -t m > "$EVIDENCE/task-22-03-auto-name.txt"
      10. tmux send-keys -t m Enter ''; sleep 3
      11. tmux send-keys -t m 'i' ''; sleep 0.5
      12. tmux send-keys -t m 'echo hello' Enter ''; sleep 1
      13. tmux capture-pane -p -t m > "$EVIDENCE/task-22-04-terminal.txt"
      14. tmux send-keys -t m C-b ''; sleep 0.5
      15. tmux send-keys -t m C-q ''; sleep 4
      16. tmux list-sessions 2>&1 | grep -q '^m:' && echo "STILL RUNNING" || echo "EXITED"
      17. cat /tmp/test-repo/.martins/state.json | jq -r '.workspaces[0].status' > "$EVIDENCE/task-22-status.txt"
      18. stty -a < /dev/tty | grep -o -e 'echo' -e '-echo' | head -1 > "$EVIDENCE/task-22-stty.txt"
    Expected Result: Step 13 screenshot contains "hello". Step 16 outputs "EXITED". task-22-status.txt contains "inactive". task-22-stty.txt contains "echo" (cooked mode — raw mode was restored).
    Evidence: $PROJECT_ROOT/.sisyphus/evidence/task-22-*.txt (6 files)

  Scenario: Graceful shutdown with 3 active PTYs
    Tool: Bash (tmux)
    Preconditions: All prior tasks complete. Invoked from project root.
    Steps:
      1. PROJECT_ROOT="$(git rev-parse --show-toplevel)" && EVIDENCE="$PROJECT_ROOT/.sisyphus/evidence" && mkdir -p "$EVIDENCE"
      2. BIN="$PROJECT_ROOT/target/release/martins"
      3. rm -rf /tmp/test-repo && mkdir -p /tmp/test-repo && cd /tmp/test-repo && git init -q && git -c user.name=test -c user.email=test@example.com commit -q --allow-empty -m init
      4. tmux kill-session -t m3 2>/dev/null; tmux new-session -d -s m3 -x 150 -y 40 "cd /tmp/test-repo && $BIN"
      5. sleep 2
      6. for i in 1 2 3; do tmux send-keys -t m3 'N' '' ; sleep 0.5; tmux send-keys -t m3 Enter ''; sleep 2; tmux send-keys -t m3 'i' ''; sleep 0.3; tmux send-keys -t m3 'sleep 3600 &' Enter ''; sleep 0.5; tmux send-keys -t m3 C-b ''; done
      7. start_ns=$(date +%s%N)
      8. tmux send-keys -t m3 C-q ''
      9. while tmux list-sessions 2>&1 | grep -q '^m3:'; do sleep 0.1; done
      10. end_ns=$(date +%s%N)
      11. echo "exit_time_ms=$(( (end_ns - start_ns) / 1000000 ))" > "$EVIDENCE/task-22-shutdown.log"
      12. ps aux | grep -v grep | grep -c "sleep 3600" >> "$EVIDENCE/task-22-shutdown.log"
    Expected Result: exit_time_ms in log < 5000. orphan count in log is 0.
    Evidence: $PROJECT_ROOT/.sisyphus/evidence/task-22-shutdown.log

  Scenario: Panic hook restores terminal
    Tool: Bash
    Preconditions: All prior tasks complete
    Steps:
      1. cargo test app::tests::panic_restores_terminal --release -- --nocapture
    Expected Result: Simulated panic in render path. Terminal disable_raw_mode called before unwinding. tty settings normal after test.
    Evidence: .sisyphus/evidence/task-22-panic.log
  ```

  **Commit**: YES — `feat(app): main event loop with graceful shutdown`

- [x] 23. **Agent selection + pre-flight at creation time**

  **What to do**:
  - Em `src/agents.rs`:
    - `impl Agent { pub fn binary_name(&self) -> &str, pub fn default_args(&self) -> Vec<String> }`
      - Opencode: binary `opencode`, args: `[]` (default → openagent)
      - Claude: binary `claude`, args: `[]`
      - Codex: binary `codex`, args: `[]`
    - `pub fn validate_available(agent: Agent) -> Result<PathBuf>`: usa `tools::detect`
  - Integração em T19 NewWorkspace modal: dropdown de agent
  - Integração em T22: ao confirmar new workspace:
    1. `validate_available(agent)` → se Err, mostra mensagem de erro no modal (sem criar worktree)
    2. Se ok: criar worktree (T10) → spawn PTY no worktree com agent binary (T11 via T18 manager)
  - State.json salva `agent` escolhido por workspace para restart coerente

  **Must NOT do**:
  - NÃO aceitar custom binary paths (só 3 agentes fixos)
  - NÃO passar flags customizadas
  - NÃO tentar auto-detectar qual agente "é melhor" para o repo

  **Recommended Agent Profile**:
  - **Category**: `deep` — Integração entre modal, worktree creation, e PTY spawn
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T24
  - **Parallel Group**: Wave 4
  - **Blocks**: T24, F1-F4
  - **Blocked By**: T6, T19

  **References**:
  - opencode CLI docs: https://opencode.ai
  - Claude Code docs: https://docs.anthropic.com/en/docs/claude-code
  - Codex CLI: https://github.com/openai/codex (check current installation)

  **WHY Each Reference Matters**:
  - Precisa confirmar nome do binário de cada agente (pode variar)
  - opencode CLI específico: nome pode ser `opencode` ou `ocode`

  **Acceptance Criteria**:
  - [ ] NewWorkspace modal mostra dropdown com 3 opções, default "opencode"
  - [ ] Agent missing → modal mostra erro "opencode not installed. Run pre-flight check."
  - [ ] Criar workspace com agent válido → PTY spawn corretamente
  - [ ] State.json registra agent escolhido
  - [ ] Re-abrir martins → workspace recarrega com agente correto (mas `Inactive` até user ativar)

  **QA Scenarios**:

  ```
  Scenario: Agent dropdown in NewWorkspace modal
    Tool: Bash
    Preconditions: All prior tasks complete
    Steps:
      1. cargo test agents::tests::new_workspace_agent_default -- --nocapture
    Expected Result: Modal initially shows agent=Opencode. Tab through options: Claude, Codex, then back to Opencode.
    Evidence: .sisyphus/evidence/task-23-dropdown.log

  Scenario: Workspace creation blocks on missing agent
    Tool: Bash
    Preconditions: All prior tasks complete. 'claude' binary NOT reachable via the test's scoped PATH lookup.
    Steps:
      1. # Test scopes PATH internally (via which::which_in or similar) — ambient cargo PATH preserved.
      2. cargo test agents::tests::missing_agent_blocks_create -- --nocapture
    Expected Result: Test invokes validate_available(Agent::Claude) with a scoped PATH that excludes 'claude'. Returns Err. Integration test asserts worktree NOT created and modal stays open with error message.
    Evidence: .sisyphus/evidence/task-23-missing.log

  Scenario: State persists agent across restarts
    Tool: Bash
    Preconditions: All prior tasks complete
    Steps:
      1. cargo test app::tests::agent_persistence -- --nocapture
    Expected Result: Create workspace with agent=Codex. Save state. Reload state. Workspace still has agent=Codex.
    Evidence: .sisyphus/evidence/task-23-persist.log
  ```

  **Commit**: YES — `feat(agents): selection with preflight validation at creation`

- [x] 24. **README + install instructions**

  **What to do**:
  - `README.md` com seções:
    - **Martins** — título + 1 linha descrição + screenshot (gerar via tmux capture no dev)
    - **What is it?** — 2 parágrafos sobre o mental model (gerente de agentes)
    - **Install** — `cargo install --path .` + requisitos (rust 1.85+, git 2.5+, opencode/bat opcionais)
    - **First run** — `cd your-repo && martins`
    - **Keybindings** — tabela com modo Normal e modo Terminal
    - **Configuration** — `$EDITOR`, `$VISUAL`, `RUST_LOG`, `.martins/state.json`
    - **Agents supported** — opencode (default), claude, codex; link para pre-flight install
    - **Status** — MVP, roadmap (v2 features: chat UI, PR creation, etc.)
    - **License** — MIT
  - `LICENSE` MIT file

  **Must NOT do**:
  - NÃO inflar README com features não implementadas
  - NÃO incluir benchmarks fake
  - NÃO escrever em português e inglês misturados (escolher inglês)

  **Recommended Agent Profile**:
  - **Category**: `writing`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T22, T23 (último para ajustar screenshots)
  - **Parallel Group**: Wave 4
  - **Blocks**: F1-F4
  - **Blocked By**: T22, T23

  **References**:
  - Exemplos de READMEs excelentes:
    - https://github.com/extrawurst/gitui/blob/master/README.md
    - https://github.com/zellij-org/zellij/blob/main/README.md
    - https://github.com/helix-editor/helix/blob/master/README.md

  **WHY Each Reference Matters**:
  - gitui, zellij, helix: READMEs de TUIs Rust consagrados com boa estrutura e screenshots

  **Acceptance Criteria**:
  - [ ] `cat README.md | wc -l` entre 80 e 250 linhas (não inflado)
  - [ ] Tem 1+ screenshot/gif
  - [ ] Todas keybindings documentadas em tabela
  - [ ] `cargo install --path .` funciona copiando do README
  - [ ] LICENSE existe e é MIT

  **QA Scenarios**:

  ```
  Scenario: README install instructions work
    Tool: Bash
    Preconditions: T24 complete
    Steps:
      1. cd /tmp && rm -rf install-test && cp -r /path/to/martins install-test && cd install-test
      2. Extract install command from README via grep
      3. Run it. Verify binary exists at ~/.cargo/bin/martins
      4. ~/.cargo/bin/martins --version
    Expected Result: Install succeeds following only README steps. Version prints.
    Evidence: .sisyphus/evidence/task-24-install.log

  Scenario: All keybindings in README match implementation
    Tool: Bash
    Preconditions: T8+T24 complete
    Steps:
      1. cargo test docs::tests::keybindings_documented -- --nocapture
    Expected Result: Test parses README keybindings table. Cross-references with Keymap::default(). Zero discrepancies.
    Evidence: .sisyphus/evidence/task-24-keys.log

  Scenario: Markdown renders without errors
    Tool: Bash
    Preconditions: T24 complete
    Steps:
      1. markdownlint README.md || echo "no markdownlint, skipping"
      2. pandoc README.md -o /tmp/readme.html 2>&1
    Expected Result: Zero lint errors. Pandoc converts without errors.
    Evidence: .sisyphus/evidence/task-24-md.log
  ```

  **Commit**: YES — `docs: readme install instructions and keybindings`

- [x] 25. **Release CI: universal macOS binary + Linux binary + GitHub Release**

  **What to do**:
  - Criar `.github/workflows/release.yml` com:
    - Trigger: `on: push: tags: - 'v*.*.*'`
    - Workflow-level `permissions: contents: write` (required for `gh release create` to write releases and upload artifacts)
    - Workflow-level `env: GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` (automatic repo-scoped token, authenticates all `gh` calls)
  - Every job MUST start with `- uses: actions/checkout@v4` to check out the source repo (otherwise Cargo.toml, LICENSE, README.md, cliff.toml, Cargo.lock, and packaging/ are not present)
  - Jobs (matrix paralela):
    - `build-macos`:
      - `runs-on: macos-14` (Apple Silicon runner)
      - Extract version: `VERSION=${GITHUB_REF_NAME#v}` (e.g. `v0.1.0` → `0.1.0`)
      - Install targets: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`
      - Build both architectures: `cargo build --release --target aarch64-apple-darwin && cargo build --release --target x86_64-apple-darwin`
      - Merge universal: `mkdir -p dist && lipo -create -output dist/martins target/aarch64-apple-darwin/release/martins target/x86_64-apple-darwin/release/martins` (binary inside archive is always named `martins`)
      - Verify: `lipo -info dist/martins` must contain "x86_64 arm64"
      - Copy LICENSE + README.md into dist/
      - Archive with versioned outer name: `tar czf martins-${VERSION}-macos-universal.tar.gz -C dist martins LICENSE README.md`
      - Compute SHA256: `shasum -a 256 martins-${VERSION}-macos-universal.tar.gz > martins-${VERSION}-macos-universal.tar.gz.sha256`
      - Upload artifact (pair: .tar.gz + .sha256)
    - `build-linux`:
      - `runs-on: ubuntu-22.04`
      - Extract version: `VERSION=${GITHUB_REF_NAME#v}`
      - Build: `cargo build --release` (x86_64 only in MVP)
      - `mkdir -p dist && cp target/release/martins dist/ && cp LICENSE README.md dist/`
      - Archive: `tar czf martins-${VERSION}-linux-x86_64.tar.gz -C dist martins LICENSE README.md`
      - SHA256 via `sha256sum`: `sha256sum martins-${VERSION}-linux-x86_64.tar.gz > martins-${VERSION}-linux-x86_64.tar.gz.sha256`
      - Upload artifact
    - `smoke-test-macos`:
      - `runs-on: macos-14`
      - `needs: build-macos`
      - Download artifact
      - Extract to a known dir: `mkdir -p "$RUNNER_TEMP/smoke" && tar xzf martins-*-macos-universal.tar.gz -C "$RUNNER_TEMP/smoke"`
      - Export absolute path: `BIN="$RUNNER_TEMP/smoke/martins" && chmod +x "$BIN"`
      - `"$BIN" --version` → stdout must contain `martins ${GITHUB_REF_NAME#v}`
      - Git repo smoke: `mkdir -p "$RUNNER_TEMP/smoke-repo" && cd "$RUNNER_TEMP/smoke-repo" && git init -q && git -c user.name=ci -c user.email=ci@example.com commit -q --allow-empty -m init`
      - `"$BIN" --help` → stdout must contain "Usage: martins" (NOTE: `--help` is a flag that prints and exits synchronously — no interactive TUI started — so NO timeout wrapper needed; the command naturally exits in milliseconds)
    - `smoke-test-linux`:
      - `runs-on: ubuntu-22.04`
      - `needs: build-linux`
      - Same smoke test, adapted for Linux artifact name. Use identical absolute-path pattern (`BIN="$RUNNER_TEMP/smoke/martins"`).
    - `changelog`:
      - `runs-on: ubuntu-22.04`
      - Instala `git-cliff` (cargo install git-cliff)
      - `git cliff --current > RELEASE_CHANGELOG.md`
      - Upload como artifact
    - `publish-release`:
      - `runs-on: ubuntu-22.04`
      - `needs: [smoke-test-macos, smoke-test-linux, changelog]`
      - Download todos artifacts
      - `gh release create ${{ github.ref_name }} --draft --title "martins ${{ github.ref_name }}" --notes-file RELEASE_CHANGELOG.md *.tar.gz *.sha256`
      - Draft release para review humano antes de publish
      - **NOTA**: Este workflow termina com release em estado DRAFT. O processo de publicação ocorre manualmente via UI do GitHub (ou `gh release edit --draft=false`). A atualização do Homebrew tap (T26) é disparada por um workflow separado acionado por `release.published`, garantindo que o tap só seja atualizado após review humano e publish explícito.
  - Adicionar `cliff.toml` na raiz: config do git-cliff (convention commits → changelog)

  **Must NOT do**:
  - NÃO assinar com Developer ID (sem Apple Developer account no MVP)
  - NÃO criar .dmg (tarball é mais simples e aceito pela comunidade Rust/CLI)
  - NÃO auto-publicar release (sempre draft → humano aprova)
  - NÃO rodar release workflow em push normal (só em tag)
  - NÃO gerar binário ARM64 Linux (só x86_64 no MVP)

  **Recommended Agent Profile**:
  - **Category**: `deep` — CI matrix + universal binary + multi-stage pipeline tem várias armadilhas (lipo, artifact passing, tag extraction)
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T24, T26
  - **Parallel Group**: Wave 4
  - **Blocks**: F1, F2, F3, F4
  - **Blocked By**: T22, T23

  **References**:
  - `lipo` usage: https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary
  - Pattern: https://github.com/atuinsh/atuin/blob/main/.github/workflows/release.yaml — production release workflow em Rust CLI
  - Pattern: https://github.com/extrawurst/gitui/blob/master/.github/workflows/cd.yml — gitui release pipeline
  - git-cliff: https://git-cliff.org/docs/
  - gh release create: https://cli.github.com/manual/gh_release_create

  **WHY Each Reference Matters**:
  - Apple doc `lipo`: único método oficial para universal binaries — comando exato e verificação com `lipo -info`
  - atuin release.yaml: pattern canônico (matrix build + artifact upload + gh release); atuin é deploy em produção por milhares de devs
  - gitui cd.yml: TUI Rust com mesmo target profile (macOS+Linux), pode copiar passo-a-passo
  - git-cliff: ferramenta padrão para changelog de Conventional Commits (nosso padrão de commits)

  **Acceptance Criteria**:
  - [ ] `release.yml` sintaxe válida (actionlint passa)
  - [ ] Workflow só dispara em tags `v*.*.*` (testar com push sem tag → sem trigger)
  - [ ] Build macOS gera universal binary (`lipo -info` mostra arm64 + x86_64)
  - [ ] Build Linux gera x86_64 binary
  - [ ] Ambos smoke tests invocam o binário empacotado e validam `--version` output
  - [ ] Release draft criada com artefatos e changelog

  **QA Scenarios**:

  ```
  Scenario: Workflow validates and triggers only on version tags
    Tool: Bash
    Preconditions: T25 complete. actionlint installed.
    Steps:
      1. actionlint .github/workflows/release.yml 2>&1 | tee /tmp/t25-lint.log
      2. grep -E "^on:" -A 5 .github/workflows/release.yml | tee /tmp/t25-triggers.log
      3. grep -q "tags:" /tmp/t25-triggers.log && grep -q "'v\*\.\*\.\*'" /tmp/t25-triggers.log && echo "trigger_ok"
    Expected Result: Step 1 exit 0 (no syntax errors). Step 3 echoes "trigger_ok". Workflow has no `push: branches` trigger.
    Evidence: .sisyphus/evidence/task-25-workflow-syntax.log

  Scenario: Universal binary build works locally (dry run of CI step)
    Tool: Bash
    Preconditions: T25 complete. Running on macOS with both rust targets installed.
    Steps:
      1. rustup target list --installed | grep -qE "^aarch64-apple-darwin$" && rustup target list --installed | grep -qE "^x86_64-apple-darwin$" || { echo "install targets first"; exit 2; }
      2. cargo build --release --target aarch64-apple-darwin 2>&1 | tail -3 | tee /tmp/t25-arm.log
      3. cargo build --release --target x86_64-apple-darwin 2>&1 | tail -3 | tee /tmp/t25-x86.log
      4. mkdir -p /tmp/t25-dist && lipo -create -output /tmp/t25-dist/martins target/aarch64-apple-darwin/release/martins target/x86_64-apple-darwin/release/martins
      5. lipo -info /tmp/t25-dist/martins | tee /tmp/t25-lipo.log
      6. /tmp/t25-dist/martins --version | tee /tmp/t25-version.log
      7. # Verify archive creation pattern produces consistent naming: inner binary "martins", outer archive versioned
      8. VERSION=0.1.0-test && cp LICENSE README.md /tmp/t25-dist/ && tar czf /tmp/martins-${VERSION}-macos-universal.tar.gz -C /tmp/t25-dist martins LICENSE README.md
      9. tar tzf /tmp/martins-${VERSION}-macos-universal.tar.gz | tee /tmp/t25-archive.log
    Expected Result: Step 5 output contains "x86_64 arm64". Step 6 outputs "martins 0.1.0". Step 9 archive listing contains exactly three entries: `martins`, `LICENSE`, `README.md` (no path prefix, binary named plainly `martins`). Build steps exit 0.
    Evidence: .sisyphus/evidence/task-25-universal.log

  Scenario: Changelog generated from conventional commits
    Tool: Bash
    Preconditions: T25 complete. git-cliff installed. Repo has conventional commits (T1-T24 already done when T25 runs; T26 commits after T25).
    Steps:
      1. git cliff --current 2>&1 | tee /tmp/t25-changelog.md
      2. wc -l /tmp/t25-changelog.md
      3. grep -qE "^## \[" /tmp/t25-changelog.md && grep -qE "^### Features|^### Bug Fixes|^### Miscellaneous" /tmp/t25-changelog.md && echo "sections_ok"
    Expected Result: Step 2 shows ≥10 lines. Step 3 echoes "sections_ok". Changelog has feat/fix/chore sections grouped.
    Evidence: .sisyphus/evidence/task-25-changelog.log
  ```

  **Commit**: YES — `ci(release): universal macos binary linux binary and github release workflow`

- [x] 26. **Homebrew tap + formula + install smoke test**

  **What to do**:
  - Documentar no README: instruções para criar o repo separado `homebrew-martins` (mesmo owner) — ESTE repo não é criado automaticamente, é setup manual uma vez.
  - Criar `packaging/homebrew/martins.rb` (template da formula) no repo do martins:
    ```ruby
    class Martins < Formula
      desc "TUI for managing AI agent teams across git worktrees"
      homepage "https://github.com/{OWNER}/martins"
      version "{VERSION}"
      license "MIT"

      on_macos do
        on_arm do
          url "https://github.com/{OWNER}/martins/releases/download/v#{version}/martins-#{version}-macos-universal.tar.gz"
          sha256 "{SHA256_MACOS}"
        end
        on_intel do
          url "https://github.com/{OWNER}/martins/releases/download/v#{version}/martins-#{version}-macos-universal.tar.gz"
          sha256 "{SHA256_MACOS}"
        end
      end

      on_linux do
        url "https://github.com/{OWNER}/martins/releases/download/v#{version}/martins-#{version}-linux-x86_64.tar.gz"
        sha256 "{SHA256_LINUX}"
      end

      def install
        bin.install "martins"
      end

      test do
        assert_match "martins #{version}", shell_output("#{bin}/martins --version")
      end
    end
    ```
  - Criar workflow **separado** `.github/workflows/homebrew-tap.yml` (NÃO adicionar ao release.yml — release.yml termina em draft, e tap só deve atualizar após publish humano):
    ```yaml
    name: Update Homebrew Tap
    on:
      release:
        types: [published]  # dispara APENAS quando release sai de draft → published (ação humana)
    permissions:
      contents: read  # precisamos ler o release (download de assets) mas não escrever neste repo
    env:
      GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    jobs:
      update-tap:
        runs-on: ubuntu-22.04
        steps:
          - name: Checkout source repo (martins)
            uses: actions/checkout@v4
            with:
              path: source  # needed to read packaging/homebrew/martins.rb template
          - name: Download release artifacts
            run: |
              mkdir -p /tmp/release-assets
              gh release download ${{ github.event.release.tag_name }} \
                --repo ${{ github.repository }} \
                --pattern '*.tar.gz*' \
                --dir /tmp/release-assets
          - name: Compute SHA256 of artifacts
            id: sha
            run: |
              SHA_MACOS=$(cat /tmp/release-assets/martins-*-macos-universal.tar.gz.sha256 | awk '{print $1}')
              SHA_LINUX=$(cat /tmp/release-assets/martins-*-linux-x86_64.tar.gz.sha256 | awk '{print $1}')
              echo "macos=$SHA_MACOS" >> $GITHUB_OUTPUT
              echo "linux=$SHA_LINUX" >> $GITHUB_OUTPUT
          - name: Checkout tap repo
            uses: actions/checkout@v4
            with:
              repository: ${{ github.repository_owner }}/homebrew-martins
              token: ${{ secrets.HOMEBREW_TAP_TOKEN }}  # PAT com write access ao tap repo
              path: tap
          - name: Update formula
            run: |
              VERSION=${{ github.event.release.tag_name }}
              VERSION=${VERSION#v}  # strip leading 'v'
              mkdir -p tap/Formula
              sed -e "s/{VERSION}/$VERSION/g" \
                  -e "s|{SHA256_MACOS}|${{ steps.sha.outputs.macos }}|g" \
                  -e "s|{SHA256_LINUX}|${{ steps.sha.outputs.linux }}|g" \
                  -e "s/{OWNER}/${{ github.repository_owner }}/g" \
                  source/packaging/homebrew/martins.rb > tap/Formula/martins.rb
          - name: Commit and push
            run: |
              cd tap
              git config user.name "github-actions[bot]"
              git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
              git add Formula/martins.rb
              git commit -m "martins ${{ github.event.release.tag_name }}"
              git push
    ```
    - Este workflow só roda quando release vai de `draft` → `published` (ação humana explícita via UI do GitHub ou `gh release edit --draft=false`), resolvendo a contradição do Momus
  - Atualizar README com seção "Install via Homebrew":
    ```
    brew tap {OWNER}/martins
    brew install martins
    ```

  **Must NOT do**:
  - NÃO criar o tap repo via CI (setup manual one-time — documentado no README)
  - NÃO submeter ao homebrew-core (tap próprio é mais simples e não exige review)
  - NÃO bundle dependências (bat, opencode, etc — formula instala só o martins; pre-flight no app guia o user)
  - NÃO adicionar job ao `release.yml` (release.yml termina em draft — o tap update fica em workflow separado `homebrew-tap.yml` disparado por `release.published`)
  - NÃO usar `GITHUB_TOKEN` (default não tem write access a outros repos — precisa PAT `HOMEBREW_TAP_TOKEN`)

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high` — Homebrew DSL + CI com tap repo tem edge cases (auth, SHA256 timing)
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES com T24, T25
  - **Parallel Group**: Wave 4
  - **Blocks**: F1, F3
  - **Blocked By**: T25 (precisa dos artefatos com SHA256)

  **References**:
  - Homebrew Formula Cookbook: https://docs.brew.sh/Formula-Cookbook
  - Pattern: https://github.com/sharkdp/homebrew-fd/blob/master/Formula/fd.rb — bat author's other tool, mesmo pattern (pre-built binary + tap próprio)
  - Pattern: https://github.com/BurntSushi/ripgrep/blob/master/pkg/brew/ripgrep-bin.rb — ripgrep bin formula
  - GitHub Actions tap update pattern: https://github.com/mislav/bump-homebrew-formula-action

  **WHY Each Reference Matters**:
  - Homebrew Cookbook: fonte oficial da DSL (`on_macos`, `on_arm`, `url`, `sha256`, `test do`)
  - fd e ripgrep formulas: mesma categoria (Rust CLI, pre-built binary via tap próprio) — pode copiar estrutura quase literal
  - bump-homebrew-formula-action: pode ser usado em vez de script custom, reduz boilerplate do job

  **Acceptance Criteria**:
  - [ ] `packaging/homebrew/martins.rb` existe e tem sintaxe Ruby válida (template com placeholders)
  - [ ] Formula contém `on_macos` + `on_linux` blocks
  - [ ] Formula tem bloco `test do` que valida `--version`
  - [ ] Workflow `.github/workflows/homebrew-tap.yml` existe separado de release.yml
  - [ ] Workflow disparado por `on: release: types: [published]` (não tag push)
  - [ ] Workflow usa secret `HOMEBREW_TAP_TOKEN` (não GITHUB_TOKEN)
  - [ ] README tem seção "Install via Homebrew" com `brew tap` + `brew install`
  - [ ] Placeholders `{VERSION}`, `{SHA256_MACOS}`, `{SHA256_LINUX}`, `{OWNER}` documentados em comentários da formula

  **QA Scenarios**:

  ```
  Scenario: Formula has valid Ruby syntax and required sections
    Tool: Bash
    Preconditions: T26 complete. Ruby installed (macOS ships with it).
    Steps:
      1. ruby -c packaging/homebrew/martins.rb 2>&1 | tee /tmp/t26-syntax.log
      2. grep -q "class Martins < Formula" packaging/homebrew/martins.rb && echo "class_ok"
      3. grep -q "on_macos do" packaging/homebrew/martins.rb && echo "macos_ok"
      4. grep -q "on_linux do" packaging/homebrew/martins.rb && echo "linux_ok"
      5. grep -q "test do" packaging/homebrew/martins.rb && echo "test_ok"
      6. grep -qE "assert_match.*martins.*--version" packaging/homebrew/martins.rb && echo "test_assertion_ok"
    Expected Result: Step 1 outputs "Syntax OK". Steps 2-6 all echo _ok markers.
    Evidence: .sisyphus/evidence/task-26-formula-syntax.log

  Scenario: Homebrew tap workflow is separate and triggered by release.published
    Tool: Bash
    Preconditions: T26 complete. actionlint installed.
    Steps:
      1. ls .github/workflows/homebrew-tap.yml                                                  # separate workflow exists
      2. ! grep -q "update-homebrew-tap\|update-tap" .github/workflows/release.yml              # NOT in release.yml
      3. actionlint .github/workflows/homebrew-tap.yml 2>&1 | tee /tmp/t26-lint.log
      4. grep -E "^on:" -A 4 .github/workflows/homebrew-tap.yml | tee /tmp/t26-trigger.log
      5. grep -q "release:" /tmp/t26-trigger.log && grep -q "published" /tmp/t26-trigger.log && echo "trigger_ok"
      6. grep -q "HOMEBREW_TAP_TOKEN" .github/workflows/homebrew-tap.yml && echo "auth_ok"
    Expected Result: Step 1 file exists. Step 2 exits 1 (correctly absent from release.yml). Step 3 exit 0 (syntax valid). Step 5 "trigger_ok". Step 6 "auth_ok".
    Evidence: .sisyphus/evidence/task-26-tap-wiring.log

  Scenario: README documents Homebrew install path
    Tool: Bash
    Preconditions: T26 complete
    Steps:
      1. grep -c "brew tap" README.md
      2. grep -c "brew install martins" README.md
    Expected Result: Step 1 ≥1. Step 2 ≥1.
    Evidence: .sisyphus/evidence/task-26-readme.log
  ```

  **Commit**: YES — `packaging(homebrew): tap formula and ci tap update job`

---

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 agentes revisores em PARALELO. TODOS devem APROVAR. Apresentar resultados consolidados ao usuário e obter "okay" explícito antes de marcar como completo.

- [x] F1. **Plan Compliance Audit** — `oracle`

  **What to do**: Ler o plano inteiro. Para cada "Must Have": verificar que implementação existe (abrir arquivo, executar comando, checar binário). Para cada "Must NOT Have": grep no codebase por padrões proibidos — rejeitar com file:line se encontrado. Checar evidências em `.sisyphus/evidence/`. Comparar deliverables contra plano.

  **QA Scenarios**:

  ```
  Scenario: All Must Haves implemented
    Tool: Bash
    Preconditions: T1-T26 complete
    Steps:
      1. ls target/release/martins                                             # binary exists
      2. cargo test --no-run 2>&1 | grep -c "test "                            # unit tests compiled
      3. grep -r "Keymap::default" src/keys.rs                                 # Normal/Terminal modes impl
      4. grep -r "ensure_gitignore" src/config.rs                              # gitignore bootstrap
      5. grep -r "repo.worktree\|Repository::worktree" src/git/worktree.rs     # git worktree CRUD
      6. grep -r "PseudoTerminal\|tui_term" src/ui/terminal.rs                 # tui-term embed
      7. grep -r "notify::Watcher\|Debouncer" src/watcher.rs                   # file watcher
      8. grep -rn "fs::rename.*state.json" src/state.rs                        # atomic write
      9. ls .github/workflows/release.yml                                      # release CI exists
      10. grep -q "lipo -create" .github/workflows/release.yml                 # universal binary step
      11. ls .github/workflows/homebrew-tap.yml                                 # separate tap workflow
      12. ls packaging/homebrew/martins.rb                                     # homebrew formula exists
      13. ls cliff.toml                                                        # changelog config
      14. ls .sisyphus/evidence/ | wc -l                                       # evidence captured
    Expected Result: Each grep and ls returns success. Evidence dir has ≥26 files. Binary exists.
    Evidence: .sisyphus/evidence/final-f1-audit.log

  Scenario: No forbidden patterns present
    Tool: Bash
    Preconditions: T1-T26 complete
    Steps:
      1. ! grep -rn "Arc<Mutex<Repository" src/                                # no shared Repository
      2. ! grep -rn "println!\|eprintln!" src/                                  # no stdio prints
      3. ! grep -rn "git2::Repository" src/app.rs src/main.rs                   # no git2 in main loop
      4. ! grep -rn "TODO\|FIXME\|XXX" src/ --include='*.rs'                    # no TODOs
      5. ! grep -rn "unimplemented!\|todo!()" src/ --include='*.rs'             # no stubs
    Expected Result: Every negated grep exits 1 (pattern not found). Zero violations.
    Evidence: .sisyphus/evidence/final-f1-negatives.log
  ```

  **Output**: `Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT: APPROVE/REJECT`

- [x] F2. **Code Quality Review** — `unspecified-high`

  **What to do**: Rodar build + lint + fmt + test. Revisar arquivos por padrões de baixa qualidade (unwrap/expect em produção, todo!, PT/EN misturado, nomes genéricos, imports não usados, código comentado, AI slop).

  **QA Scenarios**:

  ```
  Scenario: Build, lint, fmt, and test all pass
    Tool: Bash
    Preconditions: T1-T26 complete
    Steps:
      1. cargo build --release 2>&1 | tee /tmp/f2-build.log
      2. cargo clippy --all-targets -- -D warnings 2>&1 | tee /tmp/f2-clippy.log
      3. cargo fmt --check 2>&1 | tee /tmp/f2-fmt.log
      4. INSTA_UPDATE=no cargo test --all 2>&1 | tee /tmp/f2-test.log
    Expected Result: All four commands exit code 0. Test summary shows "test result: ok" with zero failures. Zero pending snapshots.
    Evidence: .sisyphus/evidence/final-f2-ci.log

  Scenario: No production unwrap/expect, no stubs, no generic names
    Tool: Bash
    Preconditions: T1-T26 complete
    Steps:
      1. count_unwrap=$(grep -rn "\.unwrap()\|\.expect(" src/ --include='*.rs' | grep -v "#\[cfg(test)\]" | grep -v "tests::" | wc -l)
      2. count_generic=$(grep -rn "\blet \(data\|result\|handler\|temp\|foo\|bar\)\b\s*=" src/ --include='*.rs' | wc -l)
      3. count_commented=$(grep -rn "^\s*//" src/ --include='*.rs' | grep -E "//\s*(let|fn|use|if|match)" | wc -l)
      4. echo "unwrap=$count_unwrap generic=$count_generic commented_code=$count_commented"
    Expected Result: count_unwrap ≤ 5 (thresholds tracked in main.rs init only), count_generic == 0, count_commented == 0.
    Evidence: .sisyphus/evidence/final-f2-quality.log
  ```

  **Output**: `Build [PASS/FAIL] | Clippy [PASS/FAIL] | Fmt [PASS/FAIL] | Tests [N pass/N fail] | Files [N clean/N issues] | VERDICT`

- [x] F3. **Real Manual QA E2E** — `unspecified-high` (via `Bash` + tmux detached sessions)

  **What to do**: Rodar do estado limpo via tmux. Executar TODO cenário QA de TODA task — seguir passos exatos, capturar evidências. Testar integração cross-task single-repo + edge cases.

  **QA Scenarios**:

  ```
  Scenario: Full single-repo integration via tmux
    Tool: Bash (tmux)
    Preconditions: T1-T26 complete. bat + opencode installed. Invoked from the martins project repo root (where `.sisyphus/evidence/` exists).
    Steps:
      1. # Capture absolute project root FIRST so evidence paths survive cwd changes
      2. PROJECT_ROOT="$(git rev-parse --show-toplevel)" && EVIDENCE="$PROJECT_ROOT/.sisyphus/evidence" && mkdir -p "$EVIDENCE"
      3. BIN="$PROJECT_ROOT/target/release/martins"
      4. mkdir -p /tmp/martins-qa && cd /tmp/martins-qa && rm -rf demo
      5. git init demo && cd demo && echo "hello" > README.md && git add -A && git -c user.name=qa -c user.email=qa@example.com commit -m "init"
      6. tmux kill-session -t qa 2>/dev/null; tmux new-session -d -s qa -x 150 -y 40 "cd /tmp/martins-qa/demo && $BIN"
      7. sleep 2; tmux send-keys -t qa 'N' ''; sleep 1
      8. tmux capture-pane -p -t qa > "$EVIDENCE/final-f3-01-modal.txt"
      9. tmux send-keys -t qa Enter ''; sleep 3
      10. tmux capture-pane -p -t qa > "$EVIDENCE/final-f3-02-workspace.txt"
      11. tmux send-keys -t qa 'i' ''; sleep 0.5; tmux send-keys -t qa 'echo ADD > newfile.txt' Enter ''; sleep 1
      12. tmux send-keys -t qa C-b ''; sleep 1
      13. tmux capture-pane -p -t qa > "$EVIDENCE/final-f3-03-changes.txt"
      14. tmux send-keys -t qa 'a' ''; sleep 1; tmux capture-pane -p -t qa > "$EVIDENCE/final-f3-04-archived.txt"
      15. tmux send-keys -t qa C-q ''; sleep 4
      16. cat /tmp/martins-qa/demo/.martins/state.json | jq -r '.workspaces[0].status' > "$EVIDENCE/final-f3-status.txt"
      17. tmux kill-session -t qa 2>/dev/null; tmux new-session -d -s qa-r -x 150 -y 40 "cd /tmp/martins-qa/demo && $BIN"; sleep 2
      18. tmux capture-pane -p -t qa-r > "$EVIDENCE/final-f3-05-persistence.txt"; tmux send-keys -t qa-r C-q ''; sleep 2; tmux kill-session -t qa-r 2>/dev/null
      19. ps aux | grep -v grep | grep -c opencode > "$EVIDENCE/final-f3-orphans.txt"
    Expected Result: 5 capture files exist in $EVIDENCE. final-f3-status.txt contains "archived". final-f3-03 contains "newfile.txt". final-f3-05 shows archived section with workspace. final-f3-orphans.txt contains "0".
    Evidence: $PROJECT_ROOT/.sisyphus/evidence/final-f3-*.txt (7 files)

  Scenario: Martins outside git repo exits with clear error
    Tool: Bash
    Preconditions: T1-T26 complete. Invoked from project root.
    Steps:
      1. PROJECT_ROOT="$(git rev-parse --show-toplevel)" && EVIDENCE="$PROJECT_ROOT/.sisyphus/evidence" && mkdir -p "$EVIDENCE"
      2. BIN="$PROJECT_ROOT/target/release/martins"
      3. cd /tmp && rm -rf not-a-repo && mkdir not-a-repo && cd not-a-repo
      4. # Capture exit code IMMEDIATELY — pipe to head would mask it via SIGPIPE/pipefail
      5. set +e; "$BIN" > "$EVIDENCE/final-f3-not-repo.stdout" 2> "$EVIDENCE/final-f3-not-repo.stderr"; exit_code=$?; set -e
      6. echo "exit=$exit_code" > "$EVIDENCE/final-f3-not-repo.log"
      7. head -3 "$EVIDENCE/final-f3-not-repo.stderr" >> "$EVIDENCE/final-f3-not-repo.log"
      8. grep -q "must be run from inside a git repository" "$EVIDENCE/final-f3-not-repo.stderr" && echo "msg_ok" >> "$EVIDENCE/final-f3-not-repo.log"
      9. test "$exit_code" -eq 2
    Expected Result: Step 9 exits 0 (exit_code was 2). Log contains "msg_ok" line. stderr contains the expected message.
    Evidence: $PROJECT_ROOT/.sisyphus/evidence/final-f3-not-repo.log (+ .stdout, .stderr)

  Scenario: Responsive layout collapses at width breakpoints
    Tool: Bash (tmux)
    Preconditions: T1-T26 complete. Test repo at /tmp/martins-qa/demo. Invoked from project root.
    Steps:
      1. PROJECT_ROOT="$(git rev-parse --show-toplevel)" && EVIDENCE="$PROJECT_ROOT/.sisyphus/evidence" && mkdir -p "$EVIDENCE"
      2. BIN="$PROJECT_ROOT/target/release/martins"
      3. tmux kill-session -t qa2 2>/dev/null; tmux new-session -d -s qa2 -x 200 -y 40 "cd /tmp/martins-qa/demo && $BIN"
      4. sleep 2; tmux capture-pane -p -t qa2 > "$EVIDENCE/final-f3-resize-wide.txt"
      5. tmux resize-window -t qa2 -x 110 -y 40; sleep 1; tmux capture-pane -p -t qa2 > "$EVIDENCE/final-f3-resize-medium.txt"
      6. tmux resize-window -t qa2 -x 85 -y 30; sleep 1; tmux capture-pane -p -t qa2 > "$EVIDENCE/final-f3-resize-narrow.txt"
      7. tmux resize-window -t qa2 -x 70 -y 24; sleep 1; tmux capture-pane -p -t qa2 > "$EVIDENCE/final-f3-resize-tiny.txt"
      8. tmux send-keys -t qa2 C-q ''; sleep 2; tmux kill-session -t qa2 2>/dev/null
      9. grep -q "Workspaces" "$EVIDENCE/final-f3-resize-wide.txt" && grep -q "Changes" "$EVIDENCE/final-f3-resize-wide.txt" && echo "wide_ok"
      10. grep -q "Workspaces" "$EVIDENCE/final-f3-resize-medium.txt" && ! grep -q "Changes" "$EVIDENCE/final-f3-resize-medium.txt" && echo "medium_ok"
      11. ! grep -q "Workspaces" "$EVIDENCE/final-f3-resize-narrow.txt" && ! grep -q "Changes" "$EVIDENCE/final-f3-resize-narrow.txt" && echo "narrow_ok"
      12. grep -q -i "resize" "$EVIDENCE/final-f3-resize-tiny.txt" && echo "tiny_ok"
    Expected Result: Steps 9-12 all echo their _ok markers. Visual breakpoints match T15 spec: ≥120 both sidebars present, 100-119 only left, 80-99 neither, <80 error message.
    Evidence: $PROJECT_ROOT/.sisyphus/evidence/final-f3-resize-*.txt (4 files)
  ```

  **Output**: `Scenarios [N/N pass] | Integration [N/N] | Edge Cases [N tested] | VERDICT`

- [x] F4. **Scope Fidelity Check** — `deep`

  **What to do**: Para cada task: ler "What to do", ler diff real (git log/diff). Verificar 1:1 — tudo na spec foi construído (sem faltar), nada além da spec foi construído (sem creep). Checar compliance com "Must NOT do". Detectar contaminação cross-task e flags de v2 vazando.

  **QA Scenarios**:

  ```
  Scenario: Every task commit maps to exactly one planned commit message
    Tool: Bash
    Preconditions: T1-T26 complete with atomic commits
    Steps:
      1. git log --format="%s" main..HEAD > /tmp/f4-actual.txt
      2. # Extract ONLY from task-level `**Commit**:` fields (single source of truth), ignoring index section
      3. awk '/^  \*\*Commit\*\*: YES/ { sub(/^.*— `/, ""); sub(/`.*$/, ""); print }' .sisyphus/plans/martins-tui.md > /tmp/f4-planned.txt
      4. # Sanity: exactly 26 planned commits (one per implementation task T1-T26)
      5. test "$(wc -l < /tmp/f4-planned.txt)" -eq 26
      6. diff <(sort -u /tmp/f4-planned.txt) <(sort -u /tmp/f4-actual.txt | grep -v "^Merge")
    Expected Result: Step 5 passes (exactly 26 planned commits — one per T1-T26). Step 6 returns zero diff (all planned commits present, no extras beyond merges).
    Evidence: .sisyphus/evidence/final-f4-commits.log

  Scenario: Cross-task file contamination check
    Tool: Bash
    Preconditions: T1-T26 complete. Git history has 26 task commits (Conventional Commits). `main` branch is the merge base.
    Steps:
      1. mkdir -p .sisyphus/evidence && : > .sisyphus/evidence/final-f4-contamination.log
      2. # Task isolation contract: each task maps to (a) a unique Conventional Commit subject PREFIX used to locate the commit, and (b) a file-path regex defining which files the commit may touch.
      3. # Both maps below are the single source of truth for F4 (mirror the `**Commit**:` field of each task).
      4. # Subject prefixes are verbatim — they MUST appear at the start of the commit subject.
      5. declare -A TASK_PREFIX=(
      6.   [T1]='chore(init):' [T2]='feat(core):' [T3]='feat(mpb):' [T4]='feat(state):'
      7.   [T5]='feat(config):' [T6]='feat(tools):' [T7]='chore(log):' [T8]='feat(keys):'
      8.   [T9]='feat(git): repo' [T10]='feat(git): worktree' [T11]='feat(pty):'
      9.   [T12]='feat(watcher):' [T13]='feat(git): diff' [T14]='feat(bootstrap):'
      10.   [T15]='feat(ui): responsive' [T16]='feat(ui): left' [T17]='feat(ui): right'
      11.   [T18]='feat(ui): embedded' [T19]='feat(ui): modals' [T20]='feat(ui): fuzzy'
      12.   [T21]='feat(ui): bat' [T22]='feat(app):' [T23]='feat(agents):'
      13.   [T24]='docs:' [T25]='ci(release):' [T26]='packaging(homebrew):'
      14. )
      15. declare -A TASK_RE=( [T1]='Cargo\.toml|rustfmt\.toml|clippy\.toml|^\.github/|^\.gitignore$|^\.sisyphus/evidence/\.gitkeep$' [T2]='^src/(app|config|state|mpb|tools|editor|keys|agents|watcher|error|git|pty|ui)|^src/main\.rs$' [T3]='^src/mpb\.rs$' [T4]='^src/state\.rs$' [T5]='^src/config\.rs$' [T6]='^src/tools\.rs$' [T7]='^src/(main|logging)\.rs$' [T8]='^src/keys\.rs$' [T9]='^src/git/repo\.rs$' [T10]='^src/git/worktree\.rs$' [T11]='^src/pty/' [T12]='^src/watcher\.rs$' [T13]='^src/git/diff\.rs$' [T14]='^src/config\.rs$' [T15]='^src/ui/(layout|mod)\.rs$' [T16]='^src/ui/sidebar_left\.rs$' [T17]='^src/ui/sidebar_right\.rs$' [T18]='^src/ui/terminal\.rs$|^src/pty/manager\.rs$' [T19]='^src/ui/modal\.rs$' [T20]='^src/ui/picker\.rs$' [T21]='^src/ui/preview\.rs$|^src/editor\.rs$' [T22]='^src/app\.rs$|^src/main\.rs$' [T23]='^src/agents\.rs$' [T24]='^README\.md$|^LICENSE$' [T25]='^\.github/workflows/release\.yml$|^cliff\.toml$' [T26]='^packaging/homebrew/|^\.github/workflows/homebrew-tap\.yml$|^README\.md$' )
      16. # Sanity: both maps must have exactly 26 keys
      17. test "${#TASK_PREFIX[@]}" -eq 26 && test "${#TASK_RE[@]}" -eq 26
      18. violations=0
      19. for T in T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 T16 T17 T18 T19 T20 T21 T22 T23 T24 T25 T26; do
      20.   # Fixed-string grep (-F) against the Conventional Commit subject prefix
      21.   commit=$(git log --format="%H %s" main..HEAD | grep -F -- " ${TASK_PREFIX[$T]}" | head -1 | awk '{print $1}')
      22.   if [ -z "$commit" ]; then
      23.     echo "MISSING $T (prefix: ${TASK_PREFIX[$T]})" >> .sisyphus/evidence/final-f4-contamination.log
      24.     violations=$((violations+1))
      25.     continue
      26.   fi
      27.   bad_files=$(git show --name-only --format="" "$commit" | grep -vE "${TASK_RE[$T]}" || true)
      28.   if [ -n "$bad_files" ]; then
      29.     echo "CONTAMINATION $T ($commit): $bad_files" >> .sisyphus/evidence/final-f4-contamination.log
      30.     violations=$((violations+1))
      31.   fi
      32. done
      33. echo "violations=$violations" >> .sisyphus/evidence/final-f4-contamination.log
      34. test "$violations" -eq 0
    Expected Result: Step 17 exits 0 (both maps have 26 entries). Step 34 exits 0 (violations == 0). Log ends with "violations=0". No CONTAMINATION or MISSING lines.
    Evidence: .sisyphus/evidence/final-f4-contamination.log

  Scenario: Forbidden v2 features absent
    Tool: Bash
    Preconditions: T1-T26 complete
    Steps:
      1. ! grep -rn "chat" src/ --include='*.rs' -i                              # no chat UI
      2. ! grep -rn "git.*commit\|git.*push\|git.*stage" src/ --include='*.rs'   # no git writes
      3. ! grep -rn "pull_request\|github_api\|linear_api" src/ --include='*.rs' # no PR/external API
      4. ! grep -rn "split.*terminal\|terminal.*split" src/ --include='*.rs'     # no terminal splits
    Expected Result: All negated greps exit 1 (patterns not found).
    Evidence: .sisyphus/evidence/final-f4-no-v2.log
  ```

  **Output**: `Tasks [N/N compliant] | Contamination [CLEAN/N issues] | Unaccounted [CLEAN/N files] | VERDICT`

---

## Commit Strategy

Commits atômicos por task. Convenção Conventional Commits.

> **SINGLE SOURCE OF TRUTH**: A mensagem exata de commit de cada task está no campo `**Commit**:` dentro da própria task (e.g. T1 em `src/...` section). A lista abaixo é apenas INDEX e DEVE bater 1:1 com esses campos. F4 verifica o match lendo APENAS os campos `**Commit**:` das tasks (não esta seção), garantindo que haja uma única fonte canônica.

Pre-commit para TODAS: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && INSTA_UPDATE=no cargo test`

Index (referência rápida — autoridade permanece nos campos `**Commit**:` das tasks):
- T1 → `chore(init): ...` | T2 → `feat(core): ...` | T3 → `feat(mpb): ...` | T4 → `feat(state): ...`
- T5 → `feat(config): ...` | T6 → `feat(tools): ...` | T7 → `chore(log): ...` | T8 → `feat(keys): ...`
- T9 → `feat(git): ...repo...` | T10 → `feat(git): ...worktree...` | T11 → `feat(pty): ...`
- T12 → `feat(watcher): ...` | T13 → `feat(git): ...diff...` | T14 → `feat(bootstrap): ...`
- T15-T21 → `feat(ui): ...` | T22 → `feat(app): ...` | T23 → `feat(agents): ...` | T24 → `docs: ...`
- T25 → `ci(release): ...` | T26 → `packaging(homebrew): ...`

---

## Success Criteria

### Verification Commands
```bash
cargo build --release                          # Expected: binary at target/release/martins
INSTA_UPDATE=no cargo test --all               # Expected: all tests pass, zero pending snapshots
cargo clippy --all-targets -- -D warnings      # Expected: zero warnings
cargo fmt --check                              # Expected: zero diffs
./target/release/martins --version             # Expected: "martins 0.1.0"
./target/release/martins --help                # Expected: single-line usage "Usage: martins\n       (Run from inside a git repository. Keybindings in README.)"
```

> **CLI surface (intentional simplicity)**: martins has NO subcommands. It accepts only `--version` / `-V` and `--help` / `-h`. Anything else ignored or errors. No config file CLI flags in MVP. This is enforced in T1 (the initial main.rs handles these before TUI init).

### Final Checklist
- [x] Todos os "Must Have" presentes
- [x] Todos os "Must NOT Have" ausentes (grep confirma)
- [x] Todos os testes passam
- [ ] Todos os snapshot tests passam com `INSTA_UPDATE=no cargo test` (snapshots committados, zero pending)
- [x] E2E smoke test completo passa (tmux)
- [x] Binário instalável via `cargo install --path .`
- [x] Release workflow `.github/workflows/release.yml` actionlint passa
- [x] Homebrew formula `packaging/homebrew/martins.rb` tem sintaxe Ruby válida
- [x] README descreve instalação (cargo install + Homebrew após primeiro release)
- [x] F1-F4 todos APPROVE
- [x] Usuário deu okay explícito
