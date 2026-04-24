---
id: SEED-001
status: dormant
planted: 2026-04-24
planted_during: v1.0 / Phase 1 (Architectural Split) — just completed
trigger_when: Phase 6 (Text Selection) is being discussed or planned; any milestone that revisits text selection / clipboard / PTY pane interaction
scope: Medium
---

# SEED-001: Tela principal deve ter seletor nativo do tmux (copy-mode)

## Why This Matters

O plano atual da **Phase 6 — Text Selection** (requisitos SEL-01 a SEL-04 em `.planning/REQUIREMENTS.md`) é implementar drag-select Ghostty-style por cima do PTY pane: mouse drag, highlight overlay, `cmd+c` via `pbcopy`, sobrevivência a streaming de output.

O risco crítico documentado para essa abordagem é **SEL-04** — manter o highlight estável quando o buffer PTY recebe novo output. Isso força lidar com scroll/reflow/repaint no nosso overlay, o que historicamente é onde text selection quebra.

Esta ideia troca a abordagem: em vez de construir overlay próprio, **delegar para o copy-mode nativo do tmux** no pane principal (Martins já usa tmux pra sessões — vide `src/tmux.rs`, `src/ui/terminal.rs`, e a integração PTY estabelecida nas Phases 01-03/04 do milestone v1.0).

**Vantagens:**
- Zero código de renderização/highlight próprio — tmux já resolve sobrevivência a streaming, scroll, reflow.
- Ganha **scrollback search** de graça (que hoje está adiado pra v2 em PROJECT.md).
- Copy-mode é battle-tested em tmux há décadas.
- `pbcopy` integra naturalmente via `set-option -g set-clipboard on` ou bind custom.

**Tradeoffs:**
- UX muda de "clique e arrasta" (Ghostty-style) para `prefix + [` + movimentos vim — é **teclado-primeiro**, não mouse.
- Contradiz a decisão atual em STATE.md: "Text selection scope = drag-select + cmd+c copy only".
- Usuário precisa aprender o modelo do tmux (ou o bind que a gente configure).
- Tmux copy-mode pode ser customizado com mouse enable (`set -g mouse on`), recuperando parte do drag-select, mas não é idêntico ao Ghostty.

## When to Surface

**Trigger:** Phase 6 (Text Selection) — antes de discutir/planejar, ou qualquer milestone futuro que mexa em text selection, clipboard, ou interação no pane principal.

Este seed deve ser apresentado durante `/gsd-new-milestone` ou `/gsd-discuss-phase 6` quando o escopo tocar em:
- Text selection no PTY main pane
- Clipboard integration (pbcopy/pbpaste)
- Scrollback search (ligado ao copy-mode do tmux)
- Qualquer revisão de SEL-01..04

## Scope Estimate

**Medium** — uma fase, provavelmente menor que a implementação Ghostty-style original.

Trabalho estimado:
- Configurar tmux copy-mode no pane Martins (set-option, keybindings, set-clipboard)
- Possivelmente adicionar um bind mais simples que `prefix + [` (ex: Ctrl-Shift-Space ou similar)
- Integrar com `pbcopy` no macOS (tmux já tem `copy-pipe`)
- Documentar o novo fluxo pro usuário
- Atualizar/descartar requisitos SEL-01..04 conforme a nova UX

Não requer overlay custom, não requer gerenciar repaint durante streaming — a maior parte do risco da Phase 6 desaparece.

## Breadcrumbs

Código e decisões relacionadas já no repo:

**Código (integração tmux + PTY existente):**
- [src/tmux.rs](src/tmux.rs) — wrapper de subprocess tmux (kill_session, new_session, send_keys, etc.)
- [src/ui/terminal.rs](src/ui/terminal.rs) — renderização do terminal pane
- [src/pty/session.rs](src/pty/session.rs) — PtySession, bytes flow
- [src/events.rs](src/events.rs) — handle_mouse (drag-select atual é capturado aqui se existir hoje)

**Requisitos e decisões:**
- [.planning/REQUIREMENTS.md:27-34](.planning/REQUIREMENTS.md:27) — SEL-01..04 definindo drag-select Ghostty-style
- [.planning/REQUIREMENTS.md:106-109](.planning/REQUIREMENTS.md:106) — traceability mapeando todos pra Phase 6
- [.planning/ROADMAP.md](.planning/ROADMAP.md) — Phase 6 Goal atual
- [.planning/PROJECT.md](.planning/PROJECT.md) — Key Decisions: "Text selection scope = drag-select + cmd+c copy only"
- [.planning/STATE.md](.planning/STATE.md) — mesma decisão registrada

## Notes

Plantado logo após fechar Phase 1 (Architectural Split) do milestone v1.0. Ainda faltam 5 phases (v2→v6) até Phase 6 ser discutida — tempo suficiente pra a decisão amadurecer. O drag-select original continua válido; este seed é uma **alternativa** a apresentar no momento do planejamento, não uma substituição automática.

Decisão final fica pro momento do `/gsd-discuss-phase 6`, quando o usuário pode pesar UX (mouse vs teclado) vs engineering cost (overlay custom vs delegar pro tmux).
