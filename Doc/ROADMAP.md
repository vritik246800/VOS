# VOS — Visual OS Shell: Roadmap

**Versão:** 0.1.0  
**Baseado em:** ROS (12 fases completas) + brainstorm `ROS_Idea/` (canvas + refs visuais)  
**Linguagem:** Rust 2024, Ratatui 0.29, Tokio 1

---

## Visão

VOS é uma TUI que funciona como **camada visual intermédia entre a CLI e a GUI
tradicional** — mas o objectivo é maior do que um conjunto de apps de terminal:
quer ser um **ambiente de trabalho completo (Desktop Environment) em modo texto**.
Corre em qualquer terminal (SSH, containers, VMs, TTY puro) sem Xorg/Wayland, e dá
uma experiência próxima de KDE/GNOME inteiramente em texto: janelas, dock, launcher,
notificações, definições, energia e apps — tudo navegável só pelo teclado.

**A régua "isto é um OS?":** um utilizador arranca o VOS num TTY e passa lá o dia
inteiro — ficheiros, código, música, web, chat, sistema, transferências — sem nunca
precisar de sair para a shell ou para uma GUI. Tudo o que falta para essa régua está
mapeado nas **Fase 6** (apps do brainstorm) e **Fase 7** (ambiente/DE) abaixo.

### Princípios
- **Terminal-first, teclado-first.** O rato é um extra, nunca um requisito.
- **Zero daemons próprios.** Envolvemos binários do sistema (`git`, `ssh`, `ffmpeg`,
  `systemctl`, `pactl`, `yt-dlp`…), nunca reimplementamos o SO.
- **Nunca bloquear o event loop.** Qualquer I/O incerto vai para Tokio/threads (§0.3 do PLAN).
- **Degradação graciosa.** Sem Kitty → half-blocks; sem systemd → fallback; sem rede →
  modo offline. O VOS nunca deixa de arrancar.

### Mapa de subsistemas do canvas (`ROS_Idea/brain storm.canvas`)

| Subsistema (nó do canvas) | Estado | Onde |
|---|---|---|
| File explore, System nav, Dashboard | ✅ | Fase 1, 3.8 |
| System monitor (htop/btop/agtop/metropolis) | ✅ | 3.3 |
| Git (gsw status-watch) | ✅ | 2.4 · **Actions/lazyactions falta** |
| Logs (lazytail) | ✅ | 2.1 |
| Settings / setting look | ✅ parcial | 3.7 · **theming dinâmico falta** |
| Notes / journal | ✅ | 6.1 |
| Music (rmpc/termusic/kew…) | ✅ | 6.4 |
| Weather / Calendar | ✅ | 6.5 |
| SSH panel | ✅ | 6.8 |
| **Browser** (chawan/amfora/carbonyl…) | ⬜ | 6.2 |
| **Transfer** (bifrost QR / torrent / downloads) | ⬜ | 6.3 |
| **Chat** (WhatsApp/Discord/Telegram TUI) | ⬜ | 6.6 |
| **Video extra** (ffmpeg TUI / youtube-tui) | ⬜ | 6.7 |
| **IDE / Code** (RIDE) | ⬜ | 6.9 |
| **Customização dinâmica** (hellwal/textfox) | ⬜ | 6.10 |
| **USB / Dispositivos externos** (mmtui/ppcp/superfile) — novo 2026-07-09 | ⬜ | 6.11 |
| **Autocomplete no terminal** (fish/carapace/inshellisense) — novo 2026-07-09 | ⬜ | 6.12 |
| **Ambiente de Desktop** (dock, workspaces, launcher, power…) | ⬜ | **Fase 7** |

---

## Fase 0 — Fundações (implementada)

- [x] Estrutura do projecto
- [x] Cargo.toml com todas as dependências
- [x] Core: EventBus, KeybindEngine, AppState
- [x] Window Manager: árvore de janelas, tiling/float/monocle
- [x] Terminal async (Tokio + crossterm event-stream)
- [x] Command Palette (Ctrl+P, fuzzy search)
- [x] Sistema de notificações global
- [x] Theme engine (TOML)
- [x] Session Manager (save/restore com SQLite)

## Fase 1 — MVP (implementada)

- [x] File Manager completo (copy/move/delete/rename/preview com confirmação)
- [x] Text Editor (syntax highlighting via syntect)
- [x] Terminal async não-bloqueante
- [x] Process Viewer (lista, CPU/RAM, kill, sort — via sysinfo)
- [x] System Monitor (CPU/RAM/disco/rede em tempo real)
- [x] Status bar global rica

## Correcções

- [x] Menu: item "Quit" abria o painel de Help em vez de sair — quando o item Config foi removido do menu, os índices em `handle_menu` (`events/input.rs`) não foram ajustados (3→Config, 4→Help, 5→Quit com apenas 5 itens). Corrigido: 3→Help, 4→Quitting.
- [x] Compilation: campos `favorites_list`, `favorites_state`, `theme_switcher_idx` em falta em `App` — adicionados.
- [x] Compilation: `render_status` chamada com 6 args quando a função pede 7 (`cpu_history: &[u64]`) — corrigido.
- [x] Compilation: `AppMode::Favorites` e `AppMode::ThemeSwitcher` não cobertos no match de `render_main` — arms adicionados.
- [x] Integração Fase 2: módulos `LogViewer`, `ServiceManager`, `NetworkPanel`, `DiskManager`, `Calculator` existiam como código mas sem `AppMode` variants, sem campos em `App`, sem render dispatch, sem input handlers e sem entradas na Command Palette — tudo integrado. `AppMode::{LogViewer,ServiceManager,NetworkPanel,DiskManager,Calculator}` adicionados a `state.rs`.
- [x] Integração Fase 4: módulos 4.1–4.5 criados por agentes em worktrees isolados e integrados no projecto principal — `AppMode::{PackageManager,SshManager,DockerPanel,CronEditor,ManViewer}` adicionados; campos em `App`; render dispatch em `main.rs`; input handlers em `events/input.rs`; 10 entradas na Command Palette.
- [x] Compilação: campos `favorites_list`, `favorites_state`, `theme_switcher_idx` em falta em `App` — adicionados em `app.rs`.
- [x] Compilação: `render_status` chamada com 6 args em vez de 7 (`cpu_history`) — corrigido em `main.rs`.
- [x] Compilação: `AppMode::Favorites` e `AppMode::ThemeSwitcher` não cobertos em `render_main` — adicionados os braços.
- [x] UX SSH Manager: `t` (teste de conectividade) corria `ssh ... -o ConnectTimeout=3` de forma **síncrona** na thread principal — a TUI ficava completamente congelada (sem repintar) até 3s por host testado, sem qualquer feedback visual. Corrigido: `SshManager::test_connectivity` passou a thread de fundo + canal (`tick_test`), com popup "A testar…" sobre o painel enquanto decorre; ao falhar, mostra a última linha do stderr do `ssh` (ou o motivo do erro) num `DialogKind::Alert` dismissível, em vez de só um ✗ silencioso. O `Enter` (ligação real) ganhou o mesmo tratamento: `App::ssh_pending_connect` mostra "A ligar a `<alias>`…" no frame imediatamente antes de suspender a TUI (antes saltava direto para o ecrã do `ssh` sem qualquer aviso), e uma ligação recusada pelo próprio `ssh` (exit code 255, ou falha ao arrancar o processo) mostra um popup de erro persistente em vez de só uma notificação de 4-6s que passava despercebida.
- [x] Bug crítico SSH: as `KeyboardEnhancementFlags` (protocolo de teclado avançado, `DISAMBIGUATE_ESCAPE_CODES`) só eram retiradas no fim do programa inteiro (`main.rs`), nunca antes de suspender a TUI para correr o `ssh` real — o cliente `ssh`/shell remoto não entende esse protocolo, pelo que setas e Ctrl+combos chegavam como sequências de escape em bruto (`^[[A^[[B^C`), corrompendo a sessão. Corrigido: `run_external_interactive` agora recebe `kbd_enhanced: bool` (espelhado em `App.kbd_enhanced`, definido a partir do `main.rs`) e faz `Pop`/`Push` das flags à volta de cada suspensão, não só no arranque/saída do programa. Corrigido também um risco de panic em `render_mini_overlay` (`.clamp(24, …)` podia ter `min > max` em terminais muito estreitos).

## Fase 2 — Módulos de Sistema

- [x] Log Viewer (journalctl + ficheiros) — `modules/logview.rs`, `ui/log_panel.rs`
  - [x] Live tail com auto-scroll (toggle) — ref: lazytail
  - [x] Filtro por nível (info/warn/error) e por texto (reusar Ctrl+F)
  - [x] Highlight de timestamps e níveis com cores do tema
  - [x] Abrir ficheiros de log externos pelo FileManager (`.log` ou nome com "log")
  - `LogSource::{File, Journalctl, AppDb, Command}` (Command reservado p/ Docker §4.3)
- [x] Service Manager (systemd — Linux only) — `modules/services.rs`, `ui/service_panel.rs`
  - lista units, start/stop/restart/enable/disable c/ confirmação, sort, filtro; fallback macOS
- [x] Network Panel (interfaces, IPs, ping) — `modules/network.rs`, `ui/network_panel.rs`
  - sparklines rx/tx, ping ao vivo, `local_ip()` reutilizável (§6.3)
- [x] Git Panel multi-painel — `plugins/git.rs` (`GitView::Workspace`)
  - [x] Painel staged/unstaged + painel de log/commits + painel de diff inline
  - [x] Branch, merge (`git_merge`), unstage (`git_unstage`)
  - [x] Menu de atalhos no rodapé (`[a]dd [u]nstage [c]ommit [b]ranches [p]ull [P]ush`)
- [x] Disk Manager (du/ncdu style) — `modules/disk.rs`, `ui/disk_panel.rs`
  - scan em background, barras proporcionais, navegação, delete c/ confirmação

## Fase 3 — UX Avançado

- [x] Theme hot-reload — watcher `notify` sobre `config/`, debounce 300ms, reload em `App::tick`
- [x] Theme switcher visual — 5 temas built-in (dark/light/neon/solarized/gruvbox), preview ao vivo, `:theme`
- [x] System Monitor visual upgrade — sparkline CPU na status bar, barra por-core no ProcessViewer, throttle 500ms
- [ ] Mouse support completo (click, scroll, drag resize)
- [x] Help contextual por módulo — `keymap_for(mode)`, `render_key_hints`, F1 abre no tab do modo actual
- [x] Favoritos no File Manager — SQLite `favorites`, `b` toggle ★, `B` popup, `:fav`
- [x] Configuração global via TUI (sem editar TOML) — 7 campos, persiste no Esc, palette+`:config`
- [x] Dashboard / home screen — Menu 3 colunas: relógio, recentes (1-5 abre), stats CPU/RAM

## Fase 4 — Integração
	
- [x] Package Manager — `modules/packages.rs`, `ui/packages_panel.rs`
  - `PkgBackend` trait; `BrewBackend` (macOS), `AptBackend`/`DnfBackend`/`PacmanBackend` (Linux)
  - tabs Installed | Search | Updates; detecção automática do gestor; 13 testes unitários
  - `AppMode::PackageManager`; integrado em `App`, render dispatch, status bar
- [x] SSH Manager — `modules/ssh.rs`, `ui/ssh_panel.rs`
  - parser `~/.ssh/config` (Host/HostName/User/Port/IdentityFile); chaves nunca lidas
  - tabela com status ✓/✗/?; teste de conectividade síncrono; 8 testes unitários
  - `AppMode::SshManager`; integrado em `App`, render dispatch
- [x] Docker Panel — `modules/docker.rs`, `ui/docker_panel.rs`
  - detecção do daemon; tabs Containers | Images | Volumes; formato tab-delimitado
  - `run_action` (start/stop/restart/rm); fallback se daemon parado; 12 testes
  - `AppMode::DockerPanel`; integrado em `App`, render dispatch
- [x] Cron Editor — `modules/cron.rs`, `ui/cron_panel.rs`
  - parser `crontab -l`; suporte `@daily/@hourly/@reboot`; comentários preservados
  - `describe_schedule` legível; backup automático `data/crontab.bak`; 10 testes
  - `AppMode::CronEditor`; integrado em `App`, render dispatch
- [x] Man Page Viewer — `modules/manpage.rs`, `ui/man_panel.rs`
  - parser backspace-overstrike (`c\bc` → Bold, `_\bc` → Underline); 8 testes
  - scroll, pesquisa com highlight, `man -k` apropos
  - `AppMode::ManViewer`; integrado em `App`, render dispatch

## Fase 5 — Polish

- [ ] Clipboard Manager (OSC 52 + xclip/pbcopy fallback)
- [ ] Acessibilidade (alto contraste, hints)
- [x] Calculator — `modules/calc.rs`, `ui/calc_panel.rs` (parser shunting-yard, popup, `:calc`)
- [ ] Documentação completa

## Fase 6 — Apps (brainstorm ROS_Idea)

### Notes  ✅ IMPLEMENTADO
- [x] Notas em markdown em pasta configurável (`settings.notes_dir`, default `~/Notes`, criada on-demand), indexadas no `app.db` (tabela `notes_index`) — `modules/notes.rs`, `ui/notes_panel.rs`
- [x] Preview de markdown estilizado em pane lateral — layout 2 zonas (lista 32% | preview 68% via `md_panel::parse_markdown`); `Enter` abre a nota no editor
- [x] Imagens embebidas — `![alt](path)` resolvido relativo ao `notes_dir`; tecla `i` abre a 1ª imagem no ImageViewer existente (pipeline Kitty/half-block). Render inline real no fluxo do texto fica fora do scope v1 (placeholder `[img]` no preview)
- [x] Pesquisa fuzzy pelas notas no Command Palette — itens dinâmicos `nota: <título>` injectados em runtime (`base_items + dynamic_items`); `:notes`/`:note` no parser + item "Notes" no palette
- Título/tags extraídos de frontmatter YAML (`title:`/`tags:`), fallback para 1º heading `#` ou nome do ficheiro; nova nota via `n` (slug + frontmatter mínimo); `F5` re-scan; help tab "Notes"; 4 testes unitários (slugify, frontmatter, fallback, parse de imagem)

### Browser (modo reader primeiro)
- [ ] Fetch da página → extrair artigo → render markdown no pane (`reqwest` + `scraper`/`readability`, `html2text`)
- [ ] Imagens via Kitty graphics (pipeline já existe); half-block fallback — ref: carbonyl
- [ ] Histórico/bookmarks no `app.db`

### Transfer system
- [ ] Enviar ficheiro para o telemóvel: servidor HTTP local + QR code no terminal (crate `qrcode`, half-blocks) — ref: bifrost
- [ ] Lista de jobs com barra de progresso (padrão ProcessViewer)
- [ ] Integração com FileManager: seleccionar → "enviar"
- [ ] Downloads em background via tokio tasks, estado no `app.db`

### Music player  ✅ IMPLEMENTADO
- [x] Biblioteca: scan de `settings.music_dir` → tabela `music_library` (lofty 0.22); drill-down Artists → Albums → Tracks
- [x] UI full-screen 3 zonas: Library+Queue (esquerda 35%) | Capa+Metadados+EQ (direita 65%) | Progress+Volume (baixo full-width)
- [x] Capa: lofty extrai art embebida, resize 300×300, renderizado como half-blocks; `image_id=3` reservado para Kitty
- [x] Fila: `a` adiciona, `d` remove; progress gauge com duração real (fix: `duration_secs` agora atribuído)
- [x] EQ bars animam com a música, decaem suavemente quando em pausa (`×0.85/tick`)
- [x] Space corrigido: `resume()` faz `sink.play()` (não reinicia); aplica em full-screen e mini-painel
- [x] Mini-painel "Dynamic Island": slide-down do topo, centrado, 30% largura, overlay flutuante sem split
  - Bordas arredondadas em baixo (`BorderType::Rounded`), topo aberto
  - "Ears" `╮` / `╭` externos ao rect para efeito notch convexo
  - Cor contextual: ciano (focado) / magenta (a tocar) / cinzento (pausa)
  - Input capturado quando focado: `Space`, `←`/`→` (prev/next track), `q` (stop), `m`/`Esc` (fechar)
  - Nome da música com marquee scroll horizontal automático
- [x] Help panel: tab "Music" com todos os atalhos do player e do mini-painel

### Weather / Calendar  ✅ IMPLEMENTADO
- [x] Painel de meteorologia **multi-cidade** estilo dashboard (API + gráficos) — `modules/weather.rs`, `ui/weather_panel.rs`
  - open-meteo.com (sem API key): forecast (temp, feels-like, humidade, vento, código WMO, 24h) via `reqwest::blocking` em threads de fundo + `mpsc`; nunca bloqueia o event loop
  - **pesquisa de cidade ao vivo**: `a` abre um overlay POR CIMA do painel (sem sair do modo Weather); a partir de 3 caracteres lista candidatos via geocoding e filtra dinamicamente; `↑/↓`+`Enter` adiciona
  - lista de cidades guardada em SQLite (`weather_cities`); `d` remove, `↑/↓` selecciona, `F5` refresh, `r` força fetch
  - toggle de unidade `u`/`c`/`f` (°C ↔ °F), preferência persistida em `settings.fahrenheit` (conversão só no render; dados/cache em °C)
  - cache JSON 15 min por coordenadas em `data/weather_cache.json` (mapa)
  - layout dashboard: lista de cidades à esquerda (entradas de 2 linhas temp+nome+condição, sem ícones minúsculos) + detalhe à direita (arte ASCII do tempo, coluna de detalhes Weather/Temp/Feels/Humidity/Wind, **temperatura grande** em bloco, **gráfico 24h pequeno** em linha braille `Chart`); estados Loading/Error/Idle
  - **arte do tempo animada** (`modules/weather_anim.rs`): sol a pulsar, nuvens a deslizar, chuva/neve a cair, trovoada a piscar, nevoeiro a oscilar — uma animação por condição WMO; `weather.anim_tick` avança no `App::tick` enquanto o painel está aberto e a cor adapta-se à condição
  - seed inicial com a localização default de `settings`; `:weather`/`:wttr` + palette "Weather"; 10 testes unitários
- [x] Calendário + tarefas — `modules/calendar.rs`, `ui/calendar_panel.rs`
  - grelha mensal **grande** estilo calcure (76% da largura): cada célula mostra o nº do dia + os eventos do dia inline (cores por evento, done riscado); fim-de-semana a magenta, hoje sublinhado, dia seleccionado destacado; meteorologia actual no título quando carregada
  - navegação (foco grelha): ←/→ mês, h/l dia, ↑/↓ semana, `t` hoje
  - **editor inline** de tarefas (foco direita, sem popup): `Tab`/`a` salta para o editor do dia seleccionado, escreve-se o título, `Enter` grava e continua a escrever, `↑/↓` selecciona tarefas existentes, `Enter` (vazio) faz toggle done, `Del` apaga, `Tab`/`Esc` grava o pendente e volta à grelha
  - mês inteiro carregado de uma vez (`db.get_tasks_between` → `calendar.month_tasks`) para render dos eventos por célula; tabela SQLite `tasks (id, date, text, done)`
  - `:calendar`/`:cal` + palette "Calendar"; 6 testes unitários (month_name, days_in_month leap, weeks, clamp de mês, set_tasks)
  - sem sync externo (CalDAV) na v1, conforme planeado

### SSH TUI — cliente interactivo  ✅ IMPLEMENTADO
- [x] `run_external_interactive(cmd, args)` em `src/terminal/mod.rs` — suspende raw mode + alternate screen + mouse capture, corre o processo, restaura tudo
- [x] `Enter` no SSH Manager abre sessão SSH real (`App::ssh_connect`); `App::force_full_redraw` força `terminal.clear()` no próximo frame para limpar artefactos da sessão suspensa
- [x] Multi-sessão: **desvio deliberado** do plano original — como a sessão SSH suspende a TUI inteira, não há concorrência real possível. Implementado como tira de tabs do histórico desta sessão (`SshManager::tabs`/`record_session_inmem`), navegável com `[`/`]`, remoção com `x`. `Ctrl+T`/`Ctrl+W` continuam reservados às tabs globais da app (já interceptados antes do dispatch por-modo) — não foram reutilizados para não colidir.
- [x] Painel SFTP (`AppMode::SftpPanel`, `modules/sftp.rs` + `ui/sftp_panel.rs`) — duas colunas local/remoto, `sftp -b -` em batch mode por thread de fundo + mpsc (nunca bloqueia o event loop), `get`/`put`, parser puro `parse_sftp_ls_output` com testes de fixture. Tecla `s` no SSH Manager abre para o host seleccionado.
- [x] Grupos e labels de hosts — `SshHost.group` populado por comentário `# group: <nome>` antes do `Host` no `~/.ssh/config`, e/ou atribuído via UI (`n` novo grupo, `a` atribuir host seleccionado) persistido em SQLite (`ssh_groups`, `ssh_host_groups`). `g` colapsa/expande o grupo do host seleccionado (o host seleccionado nunca fica escondido, mesmo num grupo colapsado).
- [x] Historial de ligações com timestamp em `app.db` (`ssh_history`, `record_ssh_session`/`get_ssh_history`) — popup `h` com `Enter` para reconectar.
- [x] Reencaminhamento de portas (tunnels) — `T` (maiúscula; `t` já é o teste de conectividade) abre o popup; `c` cria (`local_port:remote_host:remote_port`), `d` mata; tunnels usam `std::process::Child` + `try_wait()` no tick (não `tokio::process` — não há necessidade de ler stdout/stderr, só saber se o processo ainda vive).
- [x] `:ssh` no parser de comandos + tab "SSH" no Help (F1) com o keymap completo.
- [x] Formulário Adicionar/Editar ligação (`AppMode::SshConnectForm`) — barra horizontal de 4 campos (Host/Username/Password/Port), `Tab` cicla entre campos. `Tab` na lista de hosts abre o formulário em branco; `Enter` liga de imediato (com popup "A ligar…") e, se não for uma recusa de ligação, pergunta se queres gravar em `~/.ssh/config`. `e` edita o host seleccionado no mesmo formulário (pré-preenchido); `Enter` aí grava directamente, sem tentar ligar. `d` remove o host seleccionado de `~/.ssh/config` (com confirmação). O campo Password nunca é gravado nem usado para ligar — o `ssh` não tem flag não-interactiva para password, pede sempre ele próprio; o campo existe só por paridade visual com o pedido do utilizador. `~/.ssh/config` tem backup automático para `data/ssh_config.bak` antes de cada escrita (mesmo padrão do Cron Editor). Novas funções puras testadas em `modules/ssh.rs`: `append_host_block`/`update_host_block`/`remove_host_block` (9 testes).
- ref visual: `ROS_Idea/src/ssh/ssh-tui.mp4`
- **Nota de ambiente**: implementado e validado via `cargo check`/`clippy`/`test` (159 testes, 0 erros); a ligação SSH/SFTP/tunnel reais e a escrita em `~/.ssh/config` não foram testadas contra um host vivo neste ambiente de desenvolvimento (sem acesso de rede SSH no sandbox) — só por inspecção de código e testes unitários dos parsers/editores.

### Chat
- [ ] Layout 2 colunas: sidebar contactos/canais + pane de mensagens (reusar side panel animado)
- [ ] Imagens recebidas abrem no ImageViewer popup
- [ ] Input bar fixa em baixo; notificações na status bar
- [ ] Começar com protocolo aberto (IRC ou Matrix via `matrix-sdk`) antes de APIs proprietárias

### Video extra
- [ ] Painel export/convert (formato/resolução/corte com widgets) — ref: ffmpeg TUI
- [ ] Timeline com thumbnails (ffprobe/ffmpeg já no pipeline)
- [ ] Fila de conversões em background
- [ ] Modo YouTube: pesquisa via `yt-dlp --dump-json`, grelha com thumbnails — ref: youtube-tui

### IDE / Code — integrar o RIDE  (PLAN §6.9)
- [ ] O editor actual (syntect) chega para edições rápidas; falta um ambiente de
  desenvolvimento a sério. O projecto irmão **RIDE** (`rust_apps/RIDE/`) já tem LSP, DAP
  (debug), VCS, terminal integrado e plugin host em MVVM
- [ ] v1: lançar o `ride` como app externa via `run_external_interactive` (o mesmo
  mecanismo do SSH) a partir do FileManager/launcher, no ficheiro/pasta actual
- [ ] v2: cliente LSP mínimo dentro do editor do VOS (diagnósticos + autocompletar +
  goto-definition) reutilizando o modelo do RIDE
- ref: `ROS_Idea/src/IDE/RIDE.MP4`

### Customização dinâmica (pywal / hellwal)  (PLAN §6.10)
- [ ] Theme switcher já existe (5 temas fixos); falta **gerar** a paleta a partir de uma
  imagem/wallpaper (estilo pywal/hellwal) e aplicá-la ao tema, ao terminal e às apps
- [ ] Extractor de cores dominantes (k-means sobre a imagem decodificada pelo crate
  `image`, já é dep) → 8/16 cores → `Theme` em runtime, com preview ao vivo
- [ ] Exportar a paleta para `~/.cache/vos/colors.*` (shell/Xresources) para o resto do
  ambiente herdar o tema
- [ ] "Setting look": galeria de temas + wallpaper picker
- ref: `customize-hellwal-colors.gif`, `customize-textfox-tui-theme.png`, `customize-hyprdots-themeswitcher.webp`

### Git Actions (lazyactions)
- [ ] O Git Workspace (2.4) cobre status/diff/commit/branch; falta o painel de **CI/CD**:
  listar workflow runs do GitHub Actions (via `gh run list --json`), ver logs de um job,
  re-run/cancelar — estilo lazyactions
- ref: `git-lazyactions.gif`

### USB / Dispositivos externos — flash drives  (PLAN §6.11) — ideia nova do canvas, 2026-07-09
- [ ] Painel de dispositivos ligados: label, sistema de ficheiros, capacidade, usado/livre,
  estado montado/desmontado; **hot-plug** — a lista actualiza sozinha ao ligar/remover a pen
- [ ] Montar/desmontar sem root: envolver `udisksctl`/`lsblk -J` (Linux, udisks2) e
  `diskutil` (macOS); v2: eventos DBus (Linux) / DiskArbitration (macOS) em vez de polling
- [ ] `Enter` monta e abre **dual-pane PC ⇄ flash** (copiar / mover / apagar entre lados)
- [ ] Motor de transferência: cópia em blocos ~1 MiB com progresso — barra + **gráfico de
  velocidade a crescer** (sparkline MB/s, média móvel) + ETA; fila de operações canceláveis
  (pendente · activo · concluído · erro) — motor partilhado com o Transfer (6.3)
- [ ] Ejectar com segurança: sync + unmount + power-off, com confirmação
- refs: mmtui, rmount, mount.yazi, cpui, ppcp, superfile (footer), task manager do Yazi —
  `ROS_Idea/src/usb devices/`

### Autocomplete no terminal — fish-style  (PLAN §6.12) — ideia nova do canvas, 2026-07-09
- [ ] Popup por baixo do cursor a partir do 1º caractere; cada tecla **refiltra (fuzzy)**;
  `↑/↓` selecciona, `Tab`/`Enter` completa a palavra, `Esc` fecha; máx. ~8 itens com scroll
- [ ] Fontes: histórico de comandos (app.db) + binários no `$PATH` (cache em background) +
  ficheiros do cwd; v2: flags/subcomandos via specs do carapace
- [ ] **Ghost text** inline (estilo fish) com a melhor sugestão; lista só para alternativas
- refs: fish, carapace, inshellisense, reedline (IdeMenu), rustyline, nucleo —
  `ROS_Idea/src/autocomplete/`

---

## Fase 7 — Ambiente de Desktop completo (a peça que falta para ser "OS")

As Fases 0–6 dão as *apps*. A Fase 7 dá o *ambiente* que as cola num OS: a casca que
está **sempre presente** (dock, launcher, notificações, energia, workspaces) e que
transforma "um conjunto de TUIs" em "um sítio onde se vive". Detalhe de implementação
com código em `PLAN.md` §7.

- [ ] **7.1 Dock / Taskbar persistente** — barra fina sempre visível: apps abertas
  (janelas/tabs), favoritos fixos, relógio e uma *tray* de estado (rede, som, bateria,
  notificações). `Super` mostra/oculta; tecla/clique salta para a app.
- [ ] **7.2 Workspaces (desktops virtuais)** — `Super+1..9` salta de workspace,
  `Super+Shift+1..9` move a janela focada; cada workspace preserva o seu layout (reusa a
  árvore do WM). Indicador no dock.
- [ ] **7.3 Global Launcher (Spotlight/KRunner)** — evolução do Command Palette para um
  campo único: apps + ficheiros (recentes/favoritos/notas) + definições + cálculo inline
  + "ao vivo" (abrir URL, converter unidades). `Super+Space`.
- [ ] **7.4 Control Center (Definições unificadas)** — um só ecrã com categorias
  (Aparência · Rede · Som · Energia · Data/Hora · Utilizadores · Dispositivos · Atalhos),
  reaproveitando os painéis existentes (Config, Network, tema).
- [ ] **7.5 Power menu + Lock screen** — overlay com Shutdown/Reboot/Logout/Suspend/Lock
  (envolve `systemctl`/`pmset`/`loginctl`, sempre com confirmação); ecrã de bloqueio com
  relógio grande e password (hash local, nunca a password do sistema).
- [ ] **7.6 Notification Center** — histórico de notificações num painel lateral
  (contadores por app) + modo "não incomodar"; notificações passam a ser persistidas e reabríveis.
- [ ] **7.7 Trash / Reciclagem** — apagar no FileManager passa a mover para o lixo (spec
  freedesktop, restaurável); "esvaziar lixo" com confirmação. `rm` real só com `Shift+Del`.
- [ ] **7.8 Wallpaper / Backdrop** — fundo do ambiente (arte ASCII animada ou imagem via
  Kitty) por trás dos painéis; base visual e cromática do tema dinâmico (6.10).
- [ ] **7.9 Device control** — volume e saída de áudio (`pactl`/`wpctl`), Bluetooth
  (`bluetoothctl`), brilho e bateria (laptop) — na tray do dock e no Control Center.
- [ ] **7.10 Quick Settings ("system poppout")** — painel de acesso rápido (estilo quick
  settings do GNOME/Android): toggles de tema, wi-fi, som, DND, brilho, captura de ecrã —
  o nó "system poppout"/"system navigation" do canvas.
- [ ] **7.11 Onboarding / first-run** — assistente na 1ª execução: tema, pastas
  (notas/música), detecção de apps disponíveis (ffmpeg, docker, gh…) e tour de atalhos.

## Fase 8 — Distribuição

- [ ] Migração de config para `~/.config/vos/` + `~/.local/share/vos/` (crate `dirs`,
  com fallback dev e migração automática) — fazer **antes** de features que persistam caminhos novos
- [ ] `install.sh` + binário único; detecção de deps opcionais com aviso claro
- [ ] Self-update (`vos --update`) via GitHub releases
- [ ] `README` com asciinema/gif; `Doc/USER_GUIDE.md` e `Doc/ARCHITECTURE.md` (5.4)
- [ ] Empacotamento: Homebrew tap (macOS), AUR (Arch), `.deb`/`.rpm`

---

## §7 — Questões em Aberto

1. **Plugin API dinâmico** — .so em Rust é complexo; por agora plugins são built-in compilados
2. **Windows support** — crossterm suporta mas system modules não; baixa prioridade
3. **Config location** — actualmente `config/config.toml` relativo ao CWD; migrar para `~/.config/vos/`
4. **Async git** — git2 crate vs chamar binário; git2 é mais robusto mas adiciona dependência pesada
5. **Multiplexer integration** — integrar com tmux/screen ou ser o próprio multiplexer?

---

## Keybindings (referência)

| Tecla               | Acção               |
| ------------------- | ------------------- |
| `Ctrl+P`            | Command Palette     |
| `Ctrl+T`            | Nova tab            |
| `Ctrl+W`            | Fechar tab          |
| `Ctrl+1–9`          | Seleccionar tab     |
| `Ctrl+←/→`          | Redimensionar split |
| `Alt+1–9`           | Saltar para janela  |
| `Alt+Tab`           | Focus history       |
| `Tab` / `Shift+Tab` | Navegar painéis     |
| `F1`                | Help contextual     |
| `F2`                | Menu global         |
| `F5`                | Refresh             |
| `F10`               | Sair                |
| `Ctrl+C/X/V`        | Copy/Cut/Paste      |
| `Ctrl+Z`            | Undo                |
| `Ctrl+S`            | Guardar             |
| `Ctrl+F`            | Pesquisar           |
| `Esc`               | Cancelar / voltar   |

### Ambiente / Desktop (Fase 7 — planeado)

| Tecla               | Acção                          |
| ------------------- | ------------------------------ |
| `Super`             | Mostrar/ocultar dock (7.1)     |
| `Super+Space`       | Global Launcher (7.3)          |
| `Super+1–9`         | Saltar de workspace (7.2)      |
| `Super+Shift+1–9`   | Mover janela p/ workspace (7.2)|
| `Super+Q`           | Quick Settings poppout (7.10)  |
| `Super+N`           | Notification Center (7.6)      |
| `Super+L`           | Bloquear ecrã (7.5)            |
| `F10`               | Power menu (Shutdown/Reboot/…) |
| `Shift+Del`         | Apagar ficheiro sem lixo (7.7) |
