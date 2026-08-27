# VOS — PLAN.md: Guia de Implementação Detalhado

**Companheiro de:** `Doc/ROADMAP.md`
**Audiência:** agentes de código (Opus/Sonnet) e contribuidores. Cada tarefa indica
exactamente *onde* mexer, *que padrão seguir* e *como validar*. Lê a §0 antes de
qualquer tarefa — quase todas seguem a mesma receita de integração.

---

## §0 — Padrões do projecto (LER PRIMEIRO)

### 0.1 Estado real do código (correcções ao CLAUDE.md)

- O enum `AppMode` real (`src/core/state.rs:5`) é:
  `Menu | FileManager | Editor | Terminal | ProcessViewer | Git | Config | AudioPlayer | VideoPlayer | ImageViewer | PdfViewer | Help | CommandPalette | Command(String) | Dialog(DialogKind) | Quitting`.
  **Não existem** os modos `SystemMonitor` nem `Logs` referidos no CLAUDE.md — o
  System Monitor vive em `SidePanelMode::SystemMonitor` e dentro do `ProcessViewer`.
  Qualquer módulo full-screen novo exige adicionar uma variante a `AppMode`.
- `SidePanelMode` real: `None | Git | Terminal | SystemMonitor | AudioPlayer`.
- Existem três animações de painel em `AppState`: `side_pct` (painel lateral direito,
  passo 4/tick, alvo 40), `git_pct` (slide vertical full-screen, passo 8/tick, alvo 100)
  e `music_pct` (mini-painel esquerdo, alvo 35). Usa-as como referência para qualquer
  animação nova.

### 0.2 Receita: adicionar um módulo full-screen novo

Checklist na ordem correcta (usa o Git panel e o ProcessViewer como modelos):

1. **Modo** — adiciona `AppMode::NomeDoModulo` em `src/core/state.rs:5`.
2. **Lógica** — cria `src/modules/nome.rs` (struct de estado + métodos, **zero UI**);
   regista em `src/modules/mod.rs`. Segue `modules/sysmon.rs` (struct com `tick()`,
   getters) ou `modules/process.rs`.
3. **UI** — cria `src/ui/nome_panel.rs` com `pub fn render_nome(f: &mut Frame, app: &App, area: Rect)`
   (ou `&mut App` se precisares de `ListState`); regista em `src/ui/mod.rs`.
   Funções `render_*` **nunca mutam estado da aplicação** — só `ListState`/scroll.
4. **Input** — cria `fn handle_nome(app: &mut App, key: KeyEvent) -> Result<()>` em
   `src/events/input.rs` e adiciona o braço no dispatch de `handle_key`
   (`src/events/input.rs:26`). Respeita a ordem de prioridade: modais → side panel →
   keybind engine global → handler por modo.
5. **Render dispatch** — adiciona o braço `AppMode::Nome => ...` em `render_main`
   (`src/main.rs:164`).
6. **Estado no App** — adiciona o campo em `App` (`src/app.rs:35`) e inicializa em
   `App::new()` (`src/app.rs:112`).
7. **Tick** — se o módulo tem dados vivos (streams, animação, polling), adiciona a
   condição em `App::needs_tick()` (`src/app.rs:231`) **e** a chamada em `App::tick()`
   (`src/app.rs:243`). Sem isto o módulo congela quando não há input.
8. **Command Palette** — adiciona `PaletteItem` à lista em
   `src/ui/command_palette.rs:154` + variante em `PaletteAction`
   (`src/ui/command_palette.rs:18`) + braço no executor do palette em
   `handle_palette` (`src/events/input.rs:132`).
9. **Command mode (`:`)** — se fizer sentido, adiciona variante a `Command` e parsing
   em `CommandParser::parse` (`src/core/command.rs:29`) + braço em
   `App::execute_command` (`src/app.rs:265`).
10. **Menu global (opcional)** — se adicionares item a `MENU_ITEMS`
    (`src/ui/menu.rs:10`), **actualiza os índices** do `match i` em `handle_menu`
    (`src/events/input.rs:309`). ⚠️ Já houve um bug por esquecer isto (ver
    "Correcções" no ROADMAP). Melhor ainda: refactor para `MENU_ITEMS` conter o
    `AppMode` alvo em vez de índices mágicos.
11. **Help** — documenta as teclas do módulo em `src/ui/help_panel.rs`.

### 0.3 Receita: processos externos

- **Síncrono curto** (git, du, systemctl status): `std::process::Command` com
  `.output()` — padrão em `src/plugins/git.rs:154` (`git_status`).
- **Streaming/longo** (journalctl -f, ping, docker logs): `tokio::process::Command`
  com stdout piped → `mpsc::unbounded_channel` → drenar no `tick()` — padrão completo
  em `src/terminal/mod.rs` (`TerminalPane`). **Nunca** bloquear o event loop.
- **Trabalho em background com buffer partilhado**: thread + `Arc<Mutex<VecDeque>>`
  — padrão em `src/video/player.rs` (prefetch de frames).

### 0.4 Receita: persistência

Tabelas novas vão para `Database::init_tables` (`src/db/sqlite.rs:27`,
`execute_batch` com `CREATE TABLE IF NOT EXISTS`) + métodos `insert_*`/`get_*` no
mesmo ficheiro com `params![]`. Já existem: `recent_files`, `command_history`,
`app_logs`, `sessions`.

### 0.5 Receita: imagens/gráficos no terminal

- `app.use_kitty` decide o caminho. Kitty: constrói `KittyFrame`, atribui a
  `app.pending_kitty` durante o draw; `main.rs` injecta depois do `terminal.draw()`.
- **IDs de imagem estáveis**: `1` = ImageViewer, `2` = VideoPlayer. Cada novo
  consumidor de Kitty graphics reserva o próximo ID (3, 4, …) e documenta-o em
  `src/kitty.rs`.
- Fallback universal: half-blocks `▀` com fg/bg RGB (ver `ui/image_panel.rs` /
  `ui/video_panel.rs`).

### 0.6 Validação por tarefa

Para cada tarefa concluída: `cargo check` → `cargo clippy` → `cargo fmt` →
`cargo run` a partir da raiz do repositório e exercitar o fluxo manualmente.
Lógica pura (parsers, filtros, formatadores) leva testes unitários no próprio
ficheiro (`#[cfg(test)]`, padrão em `src/core/command.rs:68`).

---

## Fase 2 — Módulos de Sistema

### 2.1 Log Viewer  ✅ IMPLEMENTADO

**Objectivo:** visualizador de logs full-screen com live tail, filtros e highlight.
Fontes: ficheiros de log, `journalctl` (Linux), e a tabela `app_logs` já existente.

**Ficheiros:**
- Novo: `src/modules/logview.rs`, `src/ui/log_panel.rs`
- Alterar: `core/state.rs`, `events/input.rs`, `main.rs`, `app.rs`, `ui/mod.rs`,
  `ui/command_palette.rs`, `core/command.rs`, `events/input.rs::handle_file_manager`

**Passos:**
1. `AppMode::LogViewer` em `core/state.rs`.
2. `src/modules/logview.rs`:
   ```rust
   pub enum LogSource { File(PathBuf), Journalctl, AppDb }
   pub enum LogLevel { Trace, Debug, Info, Warn, Error, Unknown }
   pub struct LogLine { pub raw: String, pub level: LogLevel, pub ts_range: Option<Range<usize>> }
   pub struct LogViewer {
       pub source: LogSource,
       pub lines: VecDeque<LogLine>,     // cap ~10_000, pop_front quando excede
       pub scroll: usize,
       pub follow: bool,                  // auto-scroll (live tail)
       pub level_filter: Option<LogLevel>,
       pub text_filter: String,
       rx: Option<mpsc::UnboundedReceiver<String>>,
       _child: Option<tokio::process::Child>,
   }
   ```
3. **Tail de ficheiro:** abrir, fazer seek para as últimas ~1000 linhas, depois numa
   tokio task ler novas linhas (loop com `tokio::time::sleep(250ms)` + read do offset
   guardado) e enviar pelo `mpsc`. **journalctl:** `tokio::process::Command::new("journalctl")
   .args(["-f","-n","500","--no-pager"])` com stdout piped — copia o padrão de
   `TerminalPane::run` em `src/terminal/mod.rs`. Guard: `#[cfg(target_os="linux")]` /
   verificar `which journalctl`; no macOS mostrar notificação de indisponível.
4. `LogViewer::tick()`: drenar `rx` com `try_recv`, parsear nível por regex simples
   (procurar `ERROR|WARN|INFO|DEBUG|TRACE` case-insensitive) e timestamp
   (prefixo ISO-8601 ou `MMM dd HH:MM:SS`), fazer push com cap. Se `follow`,
   `scroll = len`. Registar em `App::tick()` e `needs_tick()` (vivo quando
   `follow && rx.is_some()`).
5. `ui/log_panel.rs`: `Paragraph`/lista de `Line` com spans coloridos — nível com
   `LogLevel→Color` (reusar a paleta de `GitStatus::color`, `plugins/git.rs:85`),
   timestamp em `Color::DarkGray`. Barra de rodapé com estado:
   `[f]ollow:on  [e]rr [w]arn [i]nfo  /:filtro  Esc:fechar` (padrão do Git panel).
6. Teclas em `handle_logview`: `f` toggle follow; `e/w/i` ciclo de filtro de nível;
   `/` ou `Ctrl+F` abre input de filtro (reusar `DialogKind::Input` —
   `core/state.rs:27` — como o rename do FileManager faz); `↑/↓/PgUp/PgDn` scroll
   (desliga `follow` ao fazer scroll manual, religa com `End`); `Esc` →
   `restore_mode()` e matar o child.
7. Integração FileManager: em `handle_file_manager` (`events/input.rs:345`), no
   `Enter` sobre ficheiro, se a extensão é `.log` ou o nome contém `log` →
   `LogSource::File`. Acrescentar também entrada no palette ("Log Viewer") e comando
   `:logs` no parser.

**Critérios de aceitação:** abrir um `.log` pelo FileManager mostra as últimas linhas
coloridas; com `follow` activo, `echo x >> ficheiro.log` noutra shell aparece sem
input; filtro por nível e texto funcionam em conjunto; `Esc` volta ao modo anterior
sem processos órfãos (`ps aux | grep journalctl` limpo).

---

### 2.2 Service Manager (systemd — Linux only)  ✅ IMPLEMENTADO

**Objectivo:** listar units do systemd, ver estado, start/stop/restart/enable/disable.

**Implementado:** `modules/services.rs` (`ServiceManager`, `parse_units` c/ testes,
`run_action` com hint de sudo, sem elevar), `ui/service_panel.rs` (tabela estilo
ProcessViewer, cor por estado, sidebar de detalhe; fallback "systemd não disponível"
em macOS). Teclas: `s` start / `e` enable (imediatos), `t` stop / `r` restart /
`d` disable (via `ConfirmAction::ServiceAction`), `1/2` sort, `F5` refresh,
`Ctrl+F` limpa filtro, escrever filtra. Palette "Service Manager" + `:services`.

**Ficheiros:** novo `src/modules/services.rs`, `src/ui/service_panel.rs`; integração §0.2.

**Passos:**
1. Todo o módulo atrás de `cfg!(target_os = "linux")` + verificação runtime de
   `systemctl` no PATH. Noutros sistemas: ecrã com mensagem "systemd não disponível"
   (o repo é desenvolvido em macOS — este fallback é o caminho que vais ver ao testar).
2. Dados: `systemctl list-units --type=service --all --no-pager --plain --no-legend`
   via `std::process::Command` (snapshot, não streaming). Parse por colunas:
   `UNIT LOAD ACTIVE SUB DESCRIPTION` →
   `struct ServiceUnit { name, load, active, sub, description }`. Testes unitários
   do parser com output real colado como fixture.
3. UI: tabela estilo ProcessViewer (`ui/process_panel.rs` é o modelo — colunas,
   sort, selecção). Cor por estado: `active`=verde, `failed`=vermelho, `inactive`=cinza.
4. Acções com confirmação: `s` start, `t` stop, `r` restart, `e` enable, `d` disable.
   Stop/restart/disable passam por `App::confirm_dialog` — **estende**
   `ConfirmAction` (`core/state.rs:32`) com `ServiceAction { unit: String, verb: String }`
   e trata-a em `handle_dialog` (`events/input.rs:185`). Executa
   `systemctl <verb> <unit>`; se falhar com permission denied, notificação de erro
   sugerindo correr com sudo (não tentar elevar privilégios automaticamente).
5. `F5`/`r` (Action::Refresh) recarrega a lista. Filtro de texto com `Ctrl+F`
   (mesmo padrão do 2.1).

**Critérios:** em Linux lista units reais e o restart de um serviço de teste
funciona; em macOS mostra o fallback sem crash; nenhuma acção destrutiva sem diálogo.

---

### 2.3 Network Panel  ✅ IMPLEMENTADO

**Objectivo:** interfaces de rede, IPs, throughput e ferramenta de ping.

**Ficheiros:** novo `src/modules/network.rs`, `src/ui/network_panel.rs`; integração §0.2.

**Passos:**
1. Interfaces e tráfego: `sysinfo` já é dependência — usa `sysinfo::Networks`
   (`Networks::new_with_refreshed_list()`, depois `refresh()` por tick; ver uso de
   sysinfo em `modules/sysmon.rs:65`). Por interface: nome, MAC, bytes rx/tx
   acumulados e taxa (delta entre ticks, com histórico `VecDeque<f32>` de 60 amostras
   para sparkline — mesmo padrão de `CpuSample` em `modules/sysmon.rs:5`).
2. IPs: `sysinfo` não dá IPs por interface de forma portátil — parse de
   `ifconfig` (macOS) / `ip -o addr` (Linux) com `std::process::Command`, com testes
   do parser. Alternativa aceitável: crate `local-ip-address` (leve) só para o IP
   principal + parse para o resto.
3. Ping: input de host (reusar `DialogKind::Input`), depois
   `tokio::process::Command::new("ping")` com args portáteis (`-c 5` funciona em
   macOS e Linux) e stdout piped → mpsc → mostrar linhas ao vivo num sub-painel
   (padrão `TerminalPane`). Não implementar ICMP raw em Rust (exige privilégios).
4. UI: layout em 2 zonas — lista de interfaces à esquerda (selecção com ↑/↓),
   detalhe + gráfico de taxa à direita; `p` abre o ping. Refresh contínuo só quando
   o modo está activo (mesma técnica do `ProcessViewer` em `app.rs:260`).

**Critérios:** taxas rx/tx mexem ao gerar tráfego; ping a `8.8.8.8` mostra as 5
respostas ao vivo; sem ticks de rede quando o painel está fechado (`needs_tick`).

---

### 2.4 Git Panel multi-painel (estilo lazygit)  ✅ IMPLEMENTADO

**Implementado:** `GitView::Workspace` é a view default ao abrir o painel.
Layout 3 painéis: esquerda 30% lista STAGED/UNSTAGED/UNTRACKED, direita-topo 80%
diff colorido (`+` verde / `-` vermelho / `@@` ciano), direita-baixo 20% log.
`Tab`/`BackTab` cicla painéis; borda do painel focado destacada. Rodapé fixo com
todos os atalhos. Novas operações: `git_diff_file`, `git_unstage`, `git_merge`.
`refresh_workspace()` parseia `git status --porcelain`; `load_selected_diff()`
carrega diff ao mudar selecção. Teclas: `a`/`Enter` stage, `u` unstage, `c` commit
(Input dialog), `b` branches, `p` pull, `P` push, `F5` refresh, `Esc` fecha.

---

### 2.5 Disk Manager (estilo ncdu)  ✅ IMPLEMENTADO

**Objectivo:** análise de uso de disco por directoria, navegável.

**Ficheiros:** novo `src/modules/disk.rs`, `src/ui/disk_panel.rs`; integração §0.2.

**Passos:**
1. Scan em background (directorias grandes demoram): tokio task com `spawn_blocking`
   que percorre recursivamente (usa `std::fs::read_dir`; **não** seguir symlinks;
   ignorar erros de permissão por entrada) e envia progresso parcial por mpsc:
   `enum ScanMsg { Entry { path, size, is_dir }, Done }`. Estrutura final:
   `HashMap<PathBuf, u64>` de tamanhos agregados por directoria do nível corrente.
2. Estado: `pub struct DiskManager { root, current, entries: Vec<DiskEntry>, scanning: bool, total: u64, rx, ... }`
   com `tick()` a drenar o canal (registar em `needs_tick` enquanto `scanning`).
3. UI: lista ordenada por tamanho desc., cada linha com barra proporcional
   (`█` repetido, percentagem) + tamanho humano — **reusar**
   `fs::explorer::format_size` (`fs/explorer.rs:127`). Header com path actual e
   total; spinner/contador enquanto `scanning`.
4. Navegação: `Enter` entra na directoria (re-agrega do scan já feito, sem re-scan),
   `Backspace`/`←` sobe, `d` apaga (via `ConfirmAction::DeleteFile` **já existente**
   + `std::fs::remove_dir_all`), `F5` re-scan.
5. Entrada: palette ("Disk Usage") + tecla no FileManager (`u` de usage sobre a
   directoria corrente) + `:disk` no parser.

**Critérios:** scan de `~/Documents` não congela a UI (spinner visível, app responde);
tamanhos batem com `du -sh` (±blocos); apagar pede confirmação e actualiza a lista.

---

## Fase 3 — UX Avançado

### 3.1 Theme hot-reload  ✅ IMPLEMENTADO

**Implementado:** `App` cria `notify::recommended_watcher` sobre `config/` (canal
std mpsc) em `App::new`; `App::check_config_reload()` (chamado no início de
`App::tick`) drena o canal, faz debounce de 300ms, recarrega `Settings` de
`config/config.toml`, publica `BusEvent::ThemeChanged` se o tema mudou e notifica.
TOML inválido → notificação de erro, mantém settings actuais (nunca reset).

**Objectivo:** mudanças em `config/config.toml` (e futuros ficheiros de tema)
aplicam-se sem reiniciar.

**Estado actual:** o crate `notify = "7"` **já está** no Cargo.toml mas não é usado.
`Settings::load` em `config/settings.rs:31`. Existe `BusEvent::ThemeChanged`
(publicado em `app.rs:290`).

**Passos:**
1. Em `App::new`, criar `notify::recommended_watcher` sobre `config/` com um
   `std::sync::mpsc::channel` (notify usa std mpsc); guardar `_watcher` e `rx` no `App`
   (o watcher tem de viver enquanto o App vive).
2. Em `App::tick()`: `try_recv` do canal; em evento `Modify`, debounce simples
   (ignorar eventos a <300ms do último reload), `Settings::load` de novo, substituir
   `self.settings`, publicar `BusEvent::ThemeChanged` e notificação
   "Config recarregada". Adicionar a condição em `needs_tick`? Não — eventos de
   ficheiro só precisam de ser apanhados no próximo tick natural; mas como
   `needs_tick` pode devolver false indefinidamente no Menu parado, acrescenta
   `|| true` não serve — em vez disso verifica o canal também em `handle_input` ou
   aceita latência até ao próximo evento (aceitável; documenta a escolha).
3. Se `Settings::load` falhar (TOML inválido), manter settings actuais + notificação
   de erro com a linha do parse error — nunca cair para `Default`.

**Critérios:** editar `theme = "light"` no TOML noutra janela muda o tema na app em
≤1 tick após o próximo evento; TOML inválido não rebenta nem reseta settings.

### 3.2 Theme switcher visual  ✅ IMPLEMENTADO

**Objectivo:** picker de temas com preview ao vivo das paletas.

**Passos:**
1. Pré-requisito: hoje o "tema" é só uma string `dark|light`. Criar
   `src/ui/theme.rs` com `pub struct Theme { bg, fg, accent, selection, error, warn, ok, dim: Color }`
   e `Theme::by_name(&str) -> Theme` com 4–6 temas built-in (dark, light, neon,
   solarized, gruvbox). Migrar gradualmente os `Color::` hardcoded dos panels para
   `app.theme()` (começa por `status_bar`, `menu`, `file_panel`; o resto pode migrar
   por oportunidade — documenta no código que panels novos DEVEM usar `Theme`).
2. Popup switcher (overlay como o CommandPalette, `ui/command_palette.rs` é o modelo
   de popup centrado): lista de temas à esquerda; à direita um "preview card" —
   rectângulos com as 8 cores + mini-mockup (linha de status bar falsa, item
   seleccionado falso) desenhado com os `Color` do tema sob o cursor.
3. `Enter` aplica (`Command::SetTheme` já existe e persiste — `app.rs:286`);
   `Esc` restaura o tema de entrada. Navegação ↑/↓ muda o preview imediatamente
   (preview ao vivo = aplicar o tema do cursor ao render normal é ainda melhor e
   é grátis: o render lê `app.settings.theme`).
4. Entrada: palette ("Theme Switcher") + `:theme` sem argumento abre o switcher
   (hoje `theme` exige arg — `core/command.rs:50`).

**Critérios:** abrir switcher, navegar e ver o preview mudar; `Esc` não deixa o tema
trocado; escolha persiste no TOML e sobrevive a restart.

### 3.3 System Monitor visual upgrade (btop-like)  ✅ IMPLEMENTADO

**Objectivo:** gráficos de área/braille para CPU/RAM, tema neon, sparklines na status bar.

**Estado actual:** `modules/sysmon.rs` já guarda histórico (`CpuSample`,
`VecDeque`); render em `ui/sysmon_panel.rs` e `ui/process_panel.rs`.

**Passos:**
1. Gráficos: usar o widget `ratatui::widgets::Chart`/`Dataset` com
   `Marker::Braille` para CPU/RAM (histórico 60s). Alternativa para "área cheia":
   `Sparkline` widget nativo. Sem dependências novas — ratatui 0.29 traz ambos.
2. Layout do `ProcessViewer`: zona superior com 2 charts lado a lado (CPU total +
   RAM), manter a tabela de processos em baixo. Por-core: barras horizontais
   compactas (gauge por core, `sysinfo` dá `cpus()`).
3. Tema neon: entra pelo sistema de `Theme` da tarefa 3.2 (accent magenta/cyan
   brilhantes) — implementar 3.2 primeiro.
4. Sparkline na status bar (`ui/status_bar.rs:11`): quando
   `!matches!(mode, ProcessViewer)`, render de um `Sparkline` minúsculo (8–12 células)
   com o histórico de CPU. Requer que `system_monitor.tick()` corra sempre —
   mover a chamada em `app.rs:260` para fora do `if` mas com throttle (ex.: só a
   cada 30 ticks ≈ 500ms) para não pagar o custo do refresh do sysinfo a 60fps.

**Critérios:** gráficos andam em tempo real; CPU da própria app não dispara
(verificar no próprio monitor <5%); sparkline visível na status bar em FileManager.

### 3.4 Mouse support completo

**Estado actual:** `handle_mouse` existe (`events/input.rs:926`) e
`settings.mouse_enabled` existe. Verificar se `EnableMouseCapture` é feito no setup
do terminal em `main.rs` — se não, adicioná-lo (e `DisableMouseCapture` no teardown).

**Passos:**
1. Click para seleccionar: nos panels de lista (FileManager, ProcessViewer, menus),
   converter `MouseEvent{row, column}` em índice: precisa das `Rect` do último
   render — guarda `app.last_areas: AppAreas`+rects por painel num campo escrito
   durante o render (excepção documentada à regra "render não muta": só cache de
   geometria, nunca estado de aplicação).
2. Duplo-click = `Enter` (guardar timestamp do último click, threshold 400ms).
3. Scroll wheel: `MouseEventKind::ScrollUp/Down` → mapear para `Action::Up/Down`
   (×3 linhas) no painel sob o cursor.
4. Drag resize do split: em `MouseEventKind::Down` sobre a coluna da borda do split
   (±1 célula), marcar `dragging_split = true`; em `Drag`, recalcular
   `split_pct = column * 100 / width`; em `Up`, limpar. Idem para o side panel.
5. Respeitar `settings.mouse_enabled` (toggle no Config panel já existe,
   `app.rs:446`) — quando off, ignorar tudo excepto o necessário para não capturar.

**Critérios:** click selecciona, duplo-click abre, wheel faz scroll em todos os
painéis de lista, arrastar a borda do split redimensiona suavemente.

### 3.5 Help contextual por módulo  ✅ IMPLEMENTADO

**Estado actual:** `ui/help_panel.rs` com tabs (`app.help_tab`).

**Passos:**
1. Tabela central `fn keymap_for(mode: &AppMode) -> Vec<(&str, &str)>` em
   `help_panel.rs` — fonte única para o help (e reutilizável pelos rodapés).
2. `F1` (Action::Help) passa a abrir o Help **já na tab do modo actual**: em
   `handle_key`, antes de `set_mode(Help)`, mapear `prev_mode → help_tab`.
3. Rodapé de 1 linha por módulo (como o Git panel terá na 2.4): função partilhada
   `render_key_hints(f, area, &[(key, label)])` em `help_panel.rs` ou novo
   `ui/hints.rs`, usada por FileManager, LogViewer, etc.

**Critérios:** `F1` no FileManager abre direto nos atalhos do FileManager; rodapés
consistentes em ≥3 módulos.

### 3.6 Favoritos no File Manager  ✅ IMPLEMENTADO

**Implementado:** tabela `favorites (id, path UNIQUE, added_at)` em SQLite;
`HashSet<PathBuf>` em `App` carregado no arranque; `b` toggle ★ + notificação;
`B` abre `AppMode::Favorites` popup centrado (estilo CommandPalette) com navegação
↑/↓ e `Enter` → navega para dirs, abre ficheiros; path inexistente → removido com
aviso; persiste via `db.insert_favorite` / `remove_favorite`; `:fav`/`:favorites`
no parser + item "Favorites" no palette.

**Passos:**
1. Tabela `favorites (id, path UNIQUE, added_at)` em `db/sqlite.rs` (§0.4) +
   `insert_favorite`, `remove_favorite`, `get_favorites`.
2. Tecla `b` (bookmark) em `handle_file_manager` toggle do favorito sobre a entrada
   seleccionada; estrela `★` amarela como prefixo na lista (`ui/file_panel.rs`).
3. Tecla `B` abre popup de favoritos (lista simples, `Enter` navega para o path —
   `explorer.enter_dir` para dirs, `open_file` para ficheiros; se o path já não
   existe, oferecer remoção).
4. Cache em memória (`HashSet<PathBuf>` no `FileExplorer` ou `App`) carregada no
   arranque para não fazer query SQLite por frame de render.

**Critérios:** favorito sobrevive a restart; `B`→`Enter` salta para o path; path
inexistente não crasha.

### 3.7 Configuração global via TUI  ✅ IMPLEMENTADO

**Implementado:** os 7 campos de `Settings` são editáveis (acrescentados
`notification_timeout` e `terminal_scrollback`, por presets em `←/→`);
`CONFIG_FIELDS = 7`. Persiste via `settings.save` no `Esc` (com notificação).
Entrada: palette "Settings" + `:config`/`:settings`. Nota: substituição dos
índices mágicos por enum `ConfigField` fica como melhoria futura.

**Estado actual:** `ui/config_panel.rs` + `config_cycle` (`app.rs:436`) já editam
5 campos mas **não persistem** ao sair e `AppMode::Config` não tem entrada visível.

**Passos:**
1. Cobrir os 7 campos de `Settings` (`config/settings.rs:6`) — faltam
   `notification_timeout` e `terminal_scrollback`; campos numéricos editam por
   `←/→` (incrementos) em vez de input livre.
2. Persistir: ao sair do Config (`Esc`) chamar `settings.save("config/config.toml")`
   + notificação. (Hoje só `SetTheme` grava.)
3. Substituir os índices mágicos de `config_cycle(field: usize)` por um enum
   `ConfigField` com lista ordenada — elimina a classe de bug dos índices.
4. Entrada: palette ("Settings") + item no menu global (⚠️ regra do §0.2 ponto 10)
   + `:config` no parser.
5. Quando 3.1 estiver feito, o save dispara o watcher — garantir que o reload de um
   save próprio não gera notificação duplicada (comparar conteúdo ou flag
   `self_write`).

**Critérios:** todos os campos editáveis e persistidos; reabrir a app mantém valores.

### 3.8 Dashboard / home screen  ✅ IMPLEMENTADO

**Objectivo:** substituir/estender o Menu por um dashboard com widgets.

**Estado actual:** `AppMode::Menu` + splash animado (`ui/splash.rs`, `splash_tick`).

**Passos:**
1. Manter `AppMode::Menu` como o modo; redesenhar `ui/menu.rs` para um grid 2×2 de
   cards à volta da lista de menu: relógio (chrono já é dep; `%H:%M:%S` grande com
   figlet caseiro de half-blocks ou texto normal grande), sistema (CPU/RAM/uptime —
   dados do `system_monitor`, exige o throttle da 3.3 passo 4), ficheiros recentes
   (`db.get_recent_files(5)` — já existe, `db/sqlite.rs:64`), atalhos.
2. Relógio precisa de re-render por segundo: acrescentar em `needs_tick()` a
   condição `matches!(mode, Menu)` com throttle interno (re-render a cada 60 ticks).
3. Navegação: ↑/↓ continua na lista de menu (não complicar com foco em widgets na
   v1); cards são informativos.
4. O splash reveal existente fica como animação de entrada do dashboard.

**Critérios:** relógio anda; ficheiros recentes são clicáveis via tecla numérica
(1–5 abre o n-ésimo recente); CPU idle aceitável no menu parado.

---

## Fase 4 — Integração

> Padrão comum da Fase 4: todos são wrappers de binários externos. Detectar o
> binário no arranque do módulo (`which`), mostrar fallback claro se ausente,
> snapshot síncrono para listas + streaming tokio para operações longas (§0.3).

### 4.1 Package Manager  ✅ IMPLEMENTADO

**Implementado:** `modules/packages.rs` (`PkgBackend` trait; `BrewBackend` com parsers
para `brew list --versions`, `brew search`, `brew outdated`; `AptBackend`/`DnfBackend`/
`PacmanBackend` em `#[cfg(target_os="linux")]`; `detect_backend()` testa binários em
PATH; `PackageManager` com `load_installed`, `do_search`, `load_updates`, navegação,
tabs `PkgTab`; 13 testes unitários) e `ui/packages_panel.rs` (tabs Installed/Search/
Updates, tabela nome+versão+descrição, barra de estado com nome do backend e hints).
`AppMode::PackageManager`; integrado em `App`, render dispatch em `main.rs`, label
na status bar.

**Integração completa:** `handle_packages` em `events/input.rs` (↑↓ navegar, Tab mudar
tab, F5 refresh, Char pesquisar); entrada na Command Palette "Packages"; mapeamento
"PackageManager" em `execute_palette_action`.

1. Detecção ordenada: `brew` (macOS) → `apt` → `dnf` → `pacman`. Trait interno:
   ```rust
   trait PkgBackend { fn list_installed(&self) -> Result<Vec<Pkg>>;
       fn search(&self, q: &str) -> Result<Vec<Pkg>>;
       fn install_cmd(&self, p: &str) -> Vec<String>;   // comando, não execução
       fn remove_cmd(&self, p: &str) -> Vec<String>;
       fn outdated(&self) -> Result<Vec<Pkg>>; }
   ```
   Implementar primeiro `BrewBackend` (ambiente de dev é macOS), depois os Linux.
2. UI: tabs Installed | Search | Updates (padrão `help_tab`); tabela com nome,
   versão, descrição.
3. Install/remove correm como streaming no estilo `TerminalPane` (output ao vivo
   num sub-painel), com `ConfirmAction` antes. `apt/dnf` precisam de sudo —
   detectar exit code de permissão e instruir o utilizador (não pedir password).
4. Testes unitários dos parsers de output de cada backend com fixtures.

### 4.2 SSH Manager  ✅ IMPLEMENTADO

**Implementado:** `modules/ssh.rs` (`SshHost` com alias/hostname/user/port/identity_file/
reachable; `SshManager` com `load_ssh_config`, `parse_ssh_config` (puro, testável),
navegação, `test_connectivity` síncrono; chaves nunca lidas; 8 testes unitários) e
`ui/ssh_panel.rs` (tabela Alias|Host:Port|User|Identity|Status com ✓/✗/?; mensagem
se sem hosts; hints). `AppMode::SshManager`; integrado em `App`, render dispatch.

**Integração completa:** `handle_ssh` em `events/input.rs` (↑↓ navegar, t testar
conectividade, F5 reload, Enter envia comando ssh ao terminal integrado); entrada na
Palette "SSH". Nota: suspensão real da TUI para SSH interactivo é melhoria futura.

1. Parse de `~/.ssh/config` (Host, HostName, User, Port, IdentityFile) — parser
   próprio simples com testes; **nunca ler/expor chaves privadas**, apenas paths.
2. Tabela `ssh_hosts` no SQLite para hosts extra/anotações (último uso, label).
3. UI: lista de hosts; `Enter` → **suspender a TUI** e dar exec ao ssh interactivo:
   `crossterm::terminal::disable_raw_mode` + `LeaveAlternateScreen`, correr
   `std::process::Command::new("ssh").arg(host).status()`, depois restaurar
   raw mode + `EnterAlternateScreen` + `terminal.clear()`. Este padrão
   suspender/retomar é novo no projecto — implementá-lo como
   `fn run_external_interactive(cmd) -> Result<ExitStatus>` em `main.rs` ou
   `terminal/mod.rs`, será reutilizado (4.5 man, editor externo).
4. `t` testa conectividade (`ssh -o BatchMode=yes -o ConnectTimeout=3 host true`
   em background, ícone ✓/✗).

### 4.3 Docker Panel  ✅ IMPLEMENTADO

**Implementado:** `modules/docker.rs` (`Container`, `DockerImage`, `DockerTab`;
`DockerPanel` com detecção de `docker` e daemon, `refresh_containers/images` com
formato tab-delimitado (sem serde_json), `run_action`, navegação, `next_tab`; 12
testes unitários) e `ui/docker_panel.rs` (tabs Containers/Images/Volumes, cores por
estado, fallback se daemon indisponível, error overlay). `AppMode::DockerPanel`;
integrado em `App`, render dispatch.

**Integração completa:** `handle_docker` em `events/input.rs` (↑↓ navegar, Tab mudar
tab, s/S/r start/stop/restart, F5 refresh, Esc voltar); entrada na Palette "Docker".
Nota: docker logs via LogViewer e detalhe docker inspect são melhorias futuras.

1. Backend: binário `docker` (não a crate bollard — consistente com a decisão
   git-via-binário, ver §7.4). `docker ps -a --format '{{json .}}'` → parse
   serde_json linha-a-linha. ⚠️ `serde_json` ainda não é dependência — adicionar.
2. Tabs: Containers | Images | Volumes. Acções: start/stop/restart/rm (com
   ConfirmAction), `l` logs → reusar o **LogViewer da 2.1** com
   `LogSource` novo `Command(Vec<String>)` (`docker logs -f <id>`) — desenhar 2.1
   já com isto em mente.
3. `Enter` num container: detalhe (`docker inspect`, campos principais).
4. Daemon parado: detectar no primeiro comando e mostrar ecrã de fallback.

### 4.4 Cron Editor  ✅ IMPLEMENTADO

**Implementado:** `modules/cron.rs` (`CronLine::{Job,Comment,Empty}`; `CronJob` com
5 campos + `description`; `CronEditor` com `load/write_crontab`, `describe_schedule`,
`begin_edit/save_edit/cancel_edit`, `add_new_job/delete_selected`, backup automático
`data/crontab.bak`; suporte `@daily/@hourly/@reboot/@weekly/@monthly/@yearly`; 10
testes unitários) e `ui/cron_panel.rs` (lista com colunas Schedule|Description|Command,
form de edição inline com 6 campos e preview live, indicador dirty `[*]`).
`AppMode::CronEditor`; integrado em `App`, render dispatch.

**Integração completa:** `handle_cron` em `events/input.rs` (↑↓ navegar, a adicionar,
d apagar, e editar, w gravar, F5 reload; Tab/Shift-Tab/Enter/Esc no form de edição);
entrada na Palette "Cron".

1. Ler `crontab -l` (exit 1 sem crontab = lista vazia, não erro). Parse das 5
   colunas + comando; comentários preservados como linhas opacas.
2. UI: lista de jobs; editor de entrada por campos (min/hora/dia/mês/weekday com
   presets: @hourly, @daily, intervalos) + descrição humana da expressão
   ("a cada 5 min, dias úteis") — gerador próprio com testes, sem dependência.
3. Gravar: escrever para temp file e `crontab <tmpfile>`; **backup automático** do
   crontab anterior para `data/crontab.bak` antes de cada gravação; nunca gravar
   sem `ConfirmAction`.
4. macOS: `crontab` existe e funciona — sem guard de OS necessário.

### 4.5 Man Page Viewer  ✅ IMPLEMENTADO

**Implementado:** `modules/manpage.rs` (`ManStyle::{Normal,Bold,Underline,Dim}`;
`ManSpan`, `ManLine`; `ManViewer` com `load_page` (parser backspace-overstrike
`c\x08c`→Bold, `_\x08c`→Underline, headers all-caps→Dim), `scroll_up/down`,
`scroll_page_up/down`, `search`/`next_match`/`prev_match`, `man_apropos`; 8 testes
unitários) e `ui/man_panel.rs` (render com styles Bold/Underline/Dim, highlight de
matches em Yellow/DarkGray, barra de estado com posição e contador de matches).
`AppMode::ManViewer`; integrado em `App`, render dispatch.

**Integração completa:** `handle_man` em `events/input.rs` (↑↓/PgUp/PgDn scroll,
Home/End, / pesquisar, n/N próximo/anterior match, q/Esc voltar); entrada na Palette
"Man". Nota: `:man <page>` no command parser é melhoria futura.

1. `man -P cat <page>` (ou `MANPAGER=cat`) → texto com backspaces de bold/underline
   (`x\bx`) — converter para spans ratatui (bold: `c\bc`; underline: `_\bc`).
   Parser com testes.
2. UI: reusar o padrão do **PdfViewer** (`pdf_pages: Vec<String>` + scroll,
   `app.rs:462` e `ui/pdf_panel.rs`) — generalizar para um `TextViewer` partilhado
   com título, scroll, search (`/` highlight + n/N) em vez de duplicar.
3. Entrada: `:man <page>` no parser + palette com input; índice de páginas via
   `man -k .` (apropos) com fuzzy search (fuzzy-matcher já é dep).

---

## Fase 5 — Polish

### 5.1 Clipboard Manager

1. Copy-para-sistema: OSC 52 primeiro (escrever
   `\x1b]52;c;BASE64\x07` para stdout — base64 já é dep; funciona por SSH), com
   fallback `pbcopy` (macOS) / `xclip`/`wl-copy` (Linux) via Command com stdin piped.
   Função utilitária `fn copy_to_system(text: &str)` em novo `src/clipboard.rs`.
2. Ligar ao existente: `Ctrl+C` no editor (copiar selecção) e no FileManager
   (copiar path) passam também pelo clipboard de sistema, mantendo o clipboard
   interno de ficheiros (`app.clipboard`) como está.
3. Histórico: tabela `clipboard_history` (cap 100, texto ≤64KB) + popup de histórico
   (`Ctrl+Shift+V` ou palette) onde `Enter` cola/copia de novo.

### 5.2 Acessibilidade

1. Tema high-contrast no sistema de `Theme` (3.2) — preto/branco puros, accent
   amarelo, **nunca** cor como único sinal: manter os símbolos (`M`, `★`, `▶`)
   já usados.
2. Verificar contraste dos temas existentes (texto dim sobre bg) e subir onde <4.5:1.
3. Hints: garantir rodapé de atalhos (3.5) em todos os módulos; opção
   `settings.show_hints: bool`.

### 5.3 Calculator  ✅ IMPLEMENTADO

**Implementado:** `modules/calc.rs` (`eval` shunting-yard: `+ - * / % ^`, parênteses,
unário, `sqrt/sin/cos/log`, literais `0x`/`0b`, nunca faz panic; `Calculator` com
histórico; 16 testes unitários) e `ui/calc_panel.rs` (popup centrado estilo
CommandPalette com resultado ao vivo e histórico). `AppMode::Calculator` é overlay
modal; `Enter` faz commit ao histórico; `:calc <expr>` avalia para notificação;
palette "Calculator". Sem dependências novas.

1. Popup pequeno (estilo CommandPalette): input + resultado ao vivo + histórico.
2. Parser de expressões próprio (shunting-yard: + - * / % ^ parênteses, funções
   sqrt/sin/cos/log, conversões hex/bin com prefixo 0x/0b) em
   `src/modules/calc.rs` — **com bateria de testes unitários**; sem dependência nova
   (alternativa aceitável: crate `meval`, mas o parser próprio é pequeno e didáctico).
3. `Enter` adiciona ao histórico e copia resultado (via 5.1); `:calc <expr>` avalia
   directo para notificação.

### 5.4 Documentação completa

1. `Doc/USER_GUIDE.md`: um capítulo por módulo (o que faz, teclas, screenshots de
   terminal em blocos de código).
2. `Doc/ARCHITECTURE.md`: extrair/expandir o conteúdo do CLAUDE.md (event loop,
   modos, Kitty pipeline, padrões §0 deste plano) — e **corrigir o CLAUDE.md**
   (lista de AppMode desactualizada, ver §0.1).
3. README.md raiz: pitch + gif/screenshot + instalação (deps: ffmpeg, poppler) +
   quickstart. `cargo doc` limpo nos módulos core (doc-comments nos tipos públicos).

---

## Fase 6 — Apps

### 6.1 Notes  ✅ IMPLEMENTADO

**Implementado:** `modules/notes.rs` (`NoteEntry`, `NotesManager` com `scan`,
`create_note`, navegação, preview cache, `first_image`; helpers puros `slugify`,
`parse_note_meta`, `parse_tags`, `extract_image_path`; 4 testes unitários) e
`ui/notes_panel.rs` (layout 2 zonas: lista de notas 32% à esquerda + preview md
68% à direita via `md_panel::parse_markdown`; rodapé de hints partilhado).
`AppMode::Notes`; integrado em `App` (campos `notes`, `notes_new`), render
dispatch em `main.rs`, status bar (`ICON_MODE_NOTES`).

**Integração completa:**
- `settings.notes_dir` (default `~/Notes`, expandido por `expand_home_path`,
  criado on-demand em `ensure_dir`).
- Tabela `notes_index (path, title, tags, modified_at)` em `db/sqlite.rs`
  (`upsert_note`, `get_notes`, `clear_notes_index`); sincronizada no arranque
  (`App::new`) e ao entrar/criar (`sync_notes_index`).
- `handle_notes` em `events/input.rs`: `↑/↓` navegar, `PgUp/PgDn` scroll do
  preview, `Enter` abrir no editor (`open_file`), `n` nova nota (Input dialog →
  `notes_new` → `create_note`), `i` abrir 1ª imagem embebida no ImageViewer
  (helper `open_image_file`), `F5` re-scan, `Esc` menu.
- Command Palette: item estático "Notes" + itens dinâmicos `nota: <título>`
  (`CommandPalette::set_note_items`, `base_len`) com acção `OpenFile`.
- Command mode: `:notes`/`:note` → `Command::ShowNotes` → `App::enter_notes`.
- Help: tab "Notes" (índice 7) + `keymap_for(AppMode::Notes)`.

**Notas de scope:** título/tags via frontmatter YAML (`title:`/`tags:`), fallback
1º heading `#` → nome do ficheiro. Render inline real de imagens no fluxo do texto
fica fora do scope v1 (placeholder `[img]` no preview, `i` abre no popup existente).

1. Config: `settings.notes_dir: PathBuf` (default `~/Notes`, criado on-demand).
   Tabela `notes_index (path, title, tags, modified_at)` re-sincronizada no arranque
   do módulo (scan do dir + mtime).
2. **Reusar o que existe**: editor com syntax highlight de markdown já existe
   (syntect via Buffer); `ui/md_panel.rs` (364 linhas, pulldown-cmark já é dep) já
   faz render de markdown estilizado — o módulo Notes é sobretudo *cola*:
   lista de notas (esquerda) + preview md (direita, `md_panel`) + `Enter` abre no
   editor com layout split (`PanelLayout::HSplit` já existe).
3. Nova nota: `n` → input de título → cria `<slug>.md` com frontmatter mínimo.
4. Imagens embebidas: no preview, linhas `![alt](path)` relativas ao notes_dir
   rendem placeholder clicável; `Enter` sobre ela abre o ImageViewer existente
   (popup, pipeline Kitty pronto). Render inline real de imagens no fluxo do texto
   é complexo (placement do Kitty no meio de texto scrollável) — **fora do scope v1**.
5. Pesquisa: as notas indexadas entram como `PaletteItem`s dinâmicos no
   CommandPalette (prefixo `nota:`) — exige tornar a lista do palette extensível
   em runtime (hoje é estática em `command_palette.rs:154`; muda para
   `base_items + dynamic_items`).

### 6.2 Browser (reader → html2text → adaptadores)

**Referência completa:** `ROS_Idea/src/md/browser.md` (3 níveis, mockup, keymap, fontes).
**Objectivo:** ler docs/artigos/HN/RSS sem sair do VOS. **Nunca** uma engine CSS completa.

**Ficheiros (§0.2):** novo `src/modules/browser.rs`, `src/ui/browser_panel.rs`;
`AppMode::Browser`; integração completa. Reusa `ui/md_panel.rs` (render) e o pipeline
Kitty (imagens).

**Crates:** `reqwest` (**já é dep**, blocking+rustls), `serde_json` (**já é dep**).
Novos: `scraper` (parse/query HTML) + `html2text` (fallback). Avaliar
`readability`/`dom_smoothie` para extracção de artigo — começar só com heurística
`scraper` + `html2text` e medir a qualidade antes de puxar mais peso.

**Estado (reusa os blocos do `md_panel`):**
```rust
pub enum BrowserState { Idle, Loading, Ready, Error(String) }
pub enum RenderMode { Reader, Html2Text }
pub struct Link { pub idx: usize, pub label: String, pub href: String }
pub struct Page {
    pub url: String, pub title: String,
    pub blocks: Vec<md_panel::Block>,     // reaproveita o modelo do md_panel
    pub links: Vec<Link>,
    pub images: Vec<(usize, String)>,     // (índice do bloco, url)
}
pub struct Browser {
    pub state: BrowserState,
    pub mode: RenderMode,
    pub page: Option<Page>,
    pub scroll: usize,
    pub history: Vec<String>, pub hist_pos: usize,       // back/forward
    pub link_hint: bool,                                  // modo 'f'
    rx: Option<std::sync::mpsc::Receiver<Result<Page, String>>>,
}
```

**Fetch sem bloquear (padrão §0.3 — thread + canal, como o Weather):**
```rust
pub fn open(&mut self, url: String) {
    self.state = BrowserState::Loading;
    let (tx, rx) = std::sync::mpsc::channel();
    self.rx = Some(rx);
    std::thread::spawn(move || { let _ = tx.send(fetch_and_extract(&url)); });
}
// Browser::tick() — registar em App::needs_tick enquanto Loading:
if let Some(rx) = &self.rx {
    if let Ok(msg) = rx.try_recv() {
        match msg {
            Ok(p)  => { self.state = BrowserState::Ready; self.page = Some(p); }
            Err(e) => self.state = BrowserState::Error(e),
        }
        self.rx = None;
    }
}
```

**Extracção (reader mode):** `fetch_and_extract` faz `reqwest::blocking::get`, depois
`scraper::Html::parse_document`. Heurística: escolher o `<article>` ou o maior bloco de
`<p>`; mapear `<h1..3>`→heading, `<p>`→parágrafo, `<pre>/<code>`→bloco de código
(highlight `syntect`, já é dep), `<a href>`→`Link` numerado `[n]`, `<img src>`→registar
em `images`. Resolver URLs relativos com o crate `url`. Fallback (sem artigo claro):
`html2text` sobre o `<body>`.

**Link hints (estilo amfora):** `f` liga `link_hint`; o render mostra `[1] [2] …` sobre
cada link; a próxima sequência numérica → `open(links[n].href)` (empurra o URL actual
para `history`). `H`/`L` = back/forward em `history`/`hist_pos`.

**Imagens:** ao entrar `Ready`, descarregar só as `images` **visíveis** para o pipeline
Kitty (`image_id = 10 + índice`; reservar o bloco 10–29 em `kitty.rs`); fallback
half-block. Lazy-load: nada de imagens fora do viewport.

**SQLite (§0.4) — schema completo em `browser.md`:**
```sql
CREATE TABLE browser_history  (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visited_at TEXT);
CREATE TABLE browser_bookmarks(id INTEGER PRIMARY KEY, url TEXT UNIQUE, title TEXT, tags TEXT, created_at TEXT);
CREATE TABLE browser_cache    (url TEXT PRIMARY KEY, content_md TEXT, fetched_at TEXT);  -- back/forward instantâneo
```

**Entrada:** `:open <url>` / `:browser`; palette "Browser"; `gx` → crate `open` para o
browser gráfico do sistema. Keymap vi-like (`j/k`, `d/u`, `gg/G`, `f`, `H/L`, `r`, `o`,
`b`/`B`, `yy`, `/`) — tabela completa em `browser.md`.

**Fases:** MVP (reader + scroll + `/` + `yy` + `gx`, sem imagens) → v1.1 histórico +
link-hints → v1.2 imagens + bookmarks → v2 html2text (`r`) + tabs + cache offline → v3
adaptadores (HN via API Firebase, RSS via `feed-rs`).

**Critérios:** `:open <url-de-artigo>` mostra texto legível em ≤2s sem congelar a UI;
`f`+número segue o link; `H` volta; bookmark sobrevive a restart.

### 6.3 Transfer system (QR + servidor local + downloads + torrent)

**Referência:** `ROS_Idea/src/md/transfer.md` (bifrost QR, download manager, torrent TUI).
**Objectivo:** enviar/receber ficheiros sem sair do VOS — PC↔telemóvel por QR+HTTP,
downloads em background, e (opcional) cliente torrent leve.

**Ficheiros (§0.2):** novo `src/modules/transfer.rs`, `src/ui/transfer_panel.rs`;
`AppMode::Transfer`. Integração com o FileManager (tecla `s` = send).

**Crates:** `qrcode` (**novo**; render próprio para half-blocks), `tokio` (**já**),
`reqwest` (**já**, downloads). Torrent: avaliar `librqbit` ou wrapping de
`aria2c`/`transmission-remote` — fica para v2 (ver nota).

**QR em half-blocks (2 módulos por célula, ~30 linhas, sem dep de imagem):**
```rust
use qrcode::{QrCode, Color};
/// Cada célula = 2 módulos QR verticais → devolve (topo, baixo) por célula para
/// pintar com Span fg/bg no render (invertível em qualquer tema).
pub fn qr_cells(data: &str) -> Vec<Vec<(bool, bool)>> {
    let code = QrCode::new(data.as_bytes()).expect("qr");
    let w = code.width();
    let m = code.to_colors();                       // Vec<Color> de w*w
    let dark = |x: isize, y: isize| -> bool {
        x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < w
            && m[y as usize * w + x as usize] == Color::Dark
    };
    let q = 2isize;                                 // quiet zone obrigatória
    (0..(w as isize + q * 2)).step_by(2).map(|y| {
        (0..(w as isize + q * 2)).map(|x| {
            (dark(x - q, y - q), dark(x - q, y - q + 1))   // (cima, baixo)
        }).collect()
    }).collect()
}
// No render: '▀' com fg=cor-do-módulo-de-cima, bg=cor-do-módulo-de-baixo.
```

**Servidor HTTP mínimo (1 endpoint, sem axum/hyper):**
```rust
// tokio task; token aleatório por ficheiro; desliga após 1 download ou 10 min.
async fn serve_file(path: PathBuf, token: String, tx: mpsc::Sender<TransferEvent>) -> Result<()> {
    let listener = TcpListener::bind("0.0.0.0:0").await?;          // porta livre do SO
    let port = listener.local_addr()?.port();
    let _ = tx.send(TransferEvent::Ready { port });               // → UI mostra o QR
    if let Ok((mut sock, _)) = listener.accept().await {          // 1 cliente
        // (ler request line, validar "GET /f/<token>")
        let mut file = tokio::fs::File::open(&path).await?;
        let len = file.metadata().await?.len();
        let name = path.file_name().unwrap().to_string_lossy();
        let hdr = format!("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\n\
            Content-Disposition: attachment; filename=\"{name}\"\r\n\r\n");
        sock.write_all(hdr.as_bytes()).await?;
        tokio::io::copy(&mut file, &mut sock).await?;             // stream, sem RAM
        let _ = tx.send(TransferEvent::Done);
    }
    Ok(())
}
```

**IP local para o URL:** reusar `local_ip()` do Network Panel (2.3) →
`http://<ip>:<port>/f/<token>`.

**UI:** FileManager `s` (send) → popup com **QR gigante** + URL em texto + estado
(`waiting → downloading → done`). Painel `AppMode::Transfer` lista *jobs* (envios +
downloads) com barra de progresso estilo gauge do ProcessViewer.

**SQLite (§0.4):**
```sql
CREATE TABLE transfer_jobs (id INTEGER PRIMARY KEY, kind TEXT, path TEXT, url TEXT,
  status TEXT, bytes INTEGER, total INTEGER, created_at TEXT);
```

**Downloads:** `reqwest` em streaming → escrever para disco, progresso por `mpsc` (bytes).

**Torrent (v2, opcional):** wrapping de `aria2c`/`transmission-remote` (consistente com
"envolver binários") **ou** crate `librqbit` embutida, atrás de detecção como as apps da
Fase 4. **Não** é bloqueador do MVP.

**Fases:** MVP (send por QR+HTTP de 1 ficheiro) → v1.1 lista de jobs + downloads → v1.2
receber (upload do telemóvel: `POST /up`) → v2 torrent.

**Critérios:** `s` sobre um ficheiro mostra o QR; lê-lo no telemóvel descarrega o
ficheiro; o job aparece com progresso e "done"; nada bloqueia a UI.

### 6.4 Music player rico  ✅ IMPLEMENTADO

**Implementado:** biblioteca de músicas com lofty, UI full-screen redesenhada,
mini-painel "Dynamic Island" com animação slide-down, correções de bugs.

**Ficheiros:** `src/modules/music_library.rs` (novo), `src/audio/player.rs`
(estendido), `src/ui/audio_panel.rs` (redesenhado), `src/ui/music_panel.rs`
(redesenhado), `src/ui/help_panel.rs` (tab Music), `src/app.rs`, `src/main.rs`,
`src/core/state.rs`, `src/events/input.rs`, `src/config/settings.rs`,
`src/db/sqlite.rs`.

1. **Biblioteca:** scan de `settings.music_dir` para tabela `music_library`
   (path, title, artist, album, duration_secs). Metadados: crate `lofty 0.22`.
   `MusicLibrary` com drill-down Artists → Albums → Tracks; `LibraryFocus::{Library,Queue}`.
   F5 re-scan; `scan_directory()` em `music_library.rs`; 4 testes unitários.

2. **UI full-screen** (`AppMode::AudioPlayer`): layout 3 zonas —
   - Esquerda 35%: Library (Artists/Albums/Tracks) empilhado com Queue
   - Centro/Direita 65%: capa (half-block) ou `ICON_MUSIC_NOTE` + nome/artista/álbum + EQ bars
   - Baixo full-width: progress gauge (posição/duração real) + volume gauge
   `render_audio_panel(f, area, app: &App)` — acede a `music_library` e `audio_player`.

3. **Capa:** `read_cover_art(path)` extrai art embebida via lofty, redimensiona para
   300×300 (`image` crate); renderizada como half-blocks; `image_id=3` reservado para
   Kitty (não activado ainda — half-block em produção).

4. **Fila:** `Vec<PathBuf>` + `add_to_queue()`/`remove_from_queue()` no AudioPlayer;
   `a` adiciona track seleccionada, `d` remove.

5. **Correções de bugs:**
   - Space reiniciava a música → adicionado `resume()` que chama `sink.play()` em vez
     de criar novo sink; aplicado em `handle_audio` e `SidePanelMode::AudioPlayer`.
   - Progress sempre 0% → `load_current_track_meta` agora atribui `self.duration_secs`
     (estava a ler mas não a guardar).
   - EQ bars congelavam no pause → `tick()` agora faz decay `×0.85/tick` quando parado.
   - `m` não fechava o side panel → handler `SidePanelMode::AudioPlayer` corrigido.

6. **Mini-painel "Dynamic Island"** (`ui/music_panel.rs`):
   - Animação slide-down desde o topo: `music_pct` (0→10 linhas, passo 2/tick)
     em vez de largura. Painel centrado horizontalmente (30% da largura, min 22 cols).
   - Overlay flutuante sobre o conteúdo principal — sem split horizontal.
   - Bordas `BorderType::Rounded` em baixo; topo aberto (sem borda superior).
   - "Ears" Dynamic Island: `╮` (esquerdo) e `╭` (direito) renderizados 1 célula
     fora do rect do painel, simulando as curvas convexas do notch.
   - Cor das bordas: ciano (focado) / magenta (a tocar) / cinzento escuro (pausa).
   - Captura de input quando focado (`music_panel_focused: bool` em `AppState`):
     `Space`=play/pause, `←`=prev track, `→`=next track, `q`=stop+fechar, `m`/`Esc`=fechar.
   - Nome da música com marquee scroll horizontal (offset por `eq_tick / 6`).
   - EQ bars + progress gauge + hints de teclas adaptativas à altura disponível.

7. **Help panel:** tab "Music" (índice 5) com secções Library, Queue, Playback,
   Window Controls, Mini Panel. Tab "Shortcuts" renumerada para índice 6.

**Notas de arquitectura:**
- `music_pct` (era percentagem de largura) → agora nº de linhas de altura (0–10).
- `toggle_music_panel()` em `App` define `target=10` (abrir) ou `0` (fechar) e
  `music_panel_focused`.
- `handle_mini_music_panel()` em `events/input.rs` intercepta antes do dispatch
  normal quando `music_panel_focused && music_pct_target > 0`.

### 6.5 Weather / Calendar  ✅ IMPLEMENTADO

**Implementado:** `modules/weather.rs` + `ui/weather_panel.rs` (meteorologia) e
`modules/calendar.rs` + `ui/calendar_panel.rs` (calendário + tarefas).
`AppMode::{Weather,Calendar}`; integração completa §0.2 (campos em `App`, render
dispatch em `main.rs`, handlers `handle_weather`/`handle_calendar` em
`events/input.rs`, palette "Weather"/"Calendar", `:weather`/`:wttr` e
`:calendar`/`:cal` no parser, status bar + ícones, tabs de Help "Weather"/
"Calendar"). 14 testes unitários (8 weather + 6 calendar). Acesso pelo menu fica
fora de scope (palette + comandos cobrem-no, evitando a classe de bug dos índices
do menu — §0.2 ponto 10).

1. **Weather (multi-cidade, dashboard):** API `open-meteo.com` (sem API key) via
   `reqwest::blocking` + `serde_json`, em **threads de fundo** + `std::sync::mpsc`
   (padrão §0.3, sem bloquear o event loop). `WeatherData` traz temp, feels-like,
   humidade, vento, código WMO e 24h. O utilizador mantém uma **lista de cidades**
   persistida em SQLite (`weather_cities`, `add_weather_city`/`get_weather_cities`/
   `delete_weather_city`). **Adicionar cidade = pesquisa ao vivo num overlay** (não
   um diálogo): `a` chama `search_open()` e mantém-se em `AppMode::Weather`; o
   overlay é desenhado por cima no fim de `render_weather` quando
   `weather.search.active`. A partir de 3 caracteres (`should_search`) dispara
   `geocode_search` (count=10) numa thread; `tick_search()` (em `App::tick`) drena
   os candidatos e filtra dinamicamente a cada tecla (a última query ganha); `↑/↓`
   navega, `Enter` → `weather_add_selected()` persiste + adiciona + fecha o overlay.
   `d` remove, `↑/↓` selecciona cidade; cada cidade tem fetch/estado próprio,
   `is_loading()` vivo enquanto há fetch OU pesquisa pendente. Cache JSON 15 min
   **por coordenadas** (`coord_key`) em `data/weather_cache.json`. Render dashboard:
   lista à esquerda (entradas de 2 linhas temp+nome+condição, sem ícones
   minúsculos) + detalhe à direita (arte ASCII `weather_art`, coluna de detalhes
   Weather/Temp/Feels/Humidity/Wind, **temperatura grande** em bloco de 7 linhas,
   **gráfico 24h pequeno** em `Chart`+`Marker::Braille` em vez do `Sparkline`
   gigante). Seed inicial com a localização default de `settings`. `F5` refresh
   (cache-aware), `r` força. **Toggle de unidade** `u`/`c`/`f` alterna °C ↔ °F
   (`TempUnit`); a conversão é só no render (dados e cache mantêm-se em °C) e a
   preferência persiste em `settings.fahrenheit`. **Arte do tempo animada**
   (`modules/weather_anim.rs`, `weather_frame(code, anim_tick)`): sol a pulsar,
   nuvens a deslizar, chuva/neve a cair, trovoada a piscar, nevoeiro a oscilar —
   frames por grupo WMO; `weather.anim_tick` avança em `App::tick` só enquanto o
   modo é `Weather` (logo `needs_tick` inclui `Weather`) e a cor da arte adapta-se
   à condição. 12 testes unitários. Widget no dashboard (3.8) fica como melhoria
   futura.
2. **Calendar:** grelha mensal **grande** estilo calcure (`render_grid` ocupa 76% da
   largura): cada célula renderiza o nº do dia + os eventos do dia inline (paleta de
   cores por evento, done riscado), fim-de-semana a magenta, hoje sublinhado, dia
   seleccionado com tint de fundo. O título mostra a meteorologia actual
   (`app.weather`) quando carregada. Navegação (foco grelha): `←/→` mês, `h/l` dia,
   `↑/↓` semana, `t` hoje.
   Tarefas: tabela SQLite `tasks (id, date, text, done)` (`add_task`,
   `get_tasks_between` para o mês inteiro → `calendar.month_tasks`, `toggle_task`,
   `delete_task`). **Editor inline** (sem popup, painel direito): `Tab`/`a` entra em
   edição do dia seleccionado, escreve-se o título, `Enter` grava e mantém a edição
   (várias tarefas em sequência), `↑/↓` selecciona tarefas existentes, `Enter` (buffer
   vazio) faz toggle done, `Del` apaga, `Tab`/`Esc` grava o pendente e volta à grelha
   ("gravar ao sair"). Sem sync externo na v1 (CalDAV é projecto próprio).

### 6.8 SSH TUI — cliente interactivo  ✅ IMPLEMENTADO

**Referência visual:** `ROS_Idea/src/ssh/ssh-tui.mp4` (adicionado em `da85664`)
**Base existente:** `modules/ssh.rs` (parse `~/.ssh/config`, teste de conectividade, 8 testes) + `ui/ssh_panel.rs` (tabela de hosts com status ✓/✗/?)

**Implementado:** todos os 6 passos abaixo, com os seguintes desvios deliberados
face ao texto original desta secção (decididos antes de implementar, por
limitações reais da arquitectura do projecto):

1. **`run_external_interactive`** vive em `src/terminal/mod.rs` como função
   livre (sem acesso a `Terminal<Backend>`, que só existe em `main.rs`) —
   além de raw mode + alternate screen, também faz `Disable/EnableMouseCapture`
   (o snippet original não o fazia; sem isto sequências de mouse vazam para a
   sessão SSH). `App::force_full_redraw` (consumido em `main.rs` antes do
   próximo `terminal.draw()`) substitui a chamada directa a `terminal.clear()`
   que o handler de input não consegue fazer.
2. **Multi-sessão não é literalmente Ctrl+T/Ctrl+W** — esses atalhos já são
   globais (tabs da app, interceptados antes do dispatch por-modo) e nunca
   chegariam a `handle_ssh`. Como a sessão SSH bloqueia/suspende a TUI
   inteira, não existe concorrência real entre sessões; implementado como
   `SshManager::tabs` (tira de histórico desta execução), navegação `[`/`]`,
   remoção `x`.
3. **Histórico, grupos e tunnels são overlays dentro de `render_ssh_panel`**
   (mesmo padrão do overlay de pesquisa do Weather), não novos `AppMode` —
   só o **SFTP** ganhou `AppMode::SftpPanel` próprio, por ser um ecrã
   completo de duas colunas.
4. **Tunnels usam `std::process::Child` + `try_wait()`** no tick, não
   `tokio::process` — não há necessidade de ler stdout/stderr do túnel, só
   saber se o processo `ssh -N -L ...` ainda vive.
5. **Keymap final do painel SSH** (para não colidir entre as 4 sub-features):
   `↑/↓` navegar · `Enter` ligar (sessão real) · `t` testar · `F5` recarregar
   (+ grupos/histórico) · `h` histórico · `g`/`n`/`a` grupos (colapsar,
   criar, atribuir) · `s` SFTP · `T` (maiúscula) tunnels · `[`/`]`/`x` tira
   de tabs de sessão · `Esc` voltar.

**Ficheiros:** novos `src/modules/sftp.rs`, `src/ui/sftp_panel.rs`; estendidos
`src/modules/ssh.rs`, `src/db/sqlite.rs`, `src/terminal/mod.rs`,
`src/core/state.rs`, `src/core/command.rs`, `src/app.rs`, `src/main.rs`,
`src/events/input.rs`, `src/ui/ssh_panel.rs`, `src/ui/command_palette.rs`,
`src/ui/help_panel.rs` (tab "SSH"). `:ssh` no parser de comandos.

**Validação:** `cargo check`/`clippy`/`fmt`/`test` — 150 testes a passar,
0 erros/avisos novos nos ficheiros tocados. A ligação SSH/SFTP/tunnel reais
contra um host vivo não foi exercitada neste ambiente de desenvolvimento
(sandbox sem acesso de rede SSH) — validado por inspecção de código e pelos
testes unitários dos parsers (`parse_ssh_config` com comentários de grupo,
`parse_sftp_ls_output` com fixtures de `ls -l`).

**O que faltava (resolvido nesta implementação):** sessões SSH interactivas
reais, multi-sessão, SFTP, grupos, histórico.

**Follow-up de UX (pós-implementação inicial):** o teste de conectividade
(`t`) e a ligação real (`Enter`) não davam qualquer feedback enquanto
decorriam — `t` bloqueava a TUI de forma síncrona até 3s, `Enter` saltava
directo para o ecrã do `ssh` sem aviso. Corrigido: `test_connectivity` passou
a assíncrono (thread + canal, `SshManager::tick_test`) com popup "A
testar…"; `App::ssh_pending_connect` mostra "A ligar a `<alias>`…" no frame
anterior à suspensão da TUI. Falhas reais (ssh exit 255 / erro a arrancar o
processo / stderr do teste de conectividade) mostram um `DialogKind::Alert`
dismissível com o motivo, em vez de só uma notificação transitória — ver
"Correcções" no `ROADMAP.md`.

**Bug crítico + gestão de hosts (2ª iteração de follow-up):** a causa real da
corrupção do terminal durante sessões SSH (setas/Ctrl a aparecer como
sequências de escape em bruto) eram as `KeyboardEnhancementFlags` nunca
retiradas antes de suspender a TUI — só no `main()` no fim do programa.
`run_external_interactive` ganhou um parâmetro `kbd_enhanced: bool`
(`App.kbd_enhanced`) e faz Pop/Push à volta de cada suspensão. Acrescentado
também: formulário Adicionar/Editar ligação (`AppMode::SshConnectForm`,
`SshConnForm`/`SshConnField` em `modules/ssh.rs`) — barra de 4 campos
Host/Username/Password/Port, `Tab` cicla campos; `Tab` na lista abre vazio e
liga antes de perguntar se grava, `e` edita um host existente e grava sem
ligar, `d` remove (com confirmação). Persistência directa em
`~/.ssh/config` (decisão do utilizador, não SQLite) via três funções puras
testáveis — `append_host_block`/`update_host_block`/`remove_host_block` — com
backup automático para `data/ssh_config.bak` antes de cada escrita (mesmo
padrão do Cron Editor). O campo Password nunca é gravado nem usado para
ligar (decisão do utilizador): `ssh` não tem flag de password
não-interactiva, pede sempre ele próprio.

#### Passo 0 — `run_external_interactive` (pré-requisito global)

Criar `fn run_external_interactive(cmd: &str, args: &[&str]) -> Result<std::process::ExitStatus>`
em `src/terminal/mod.rs` (ou `src/main.rs` como função livre):

```rust
pub fn run_external_interactive(cmd: &str, args: &[&str]) -> Result<ExitStatus> {
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
    let status = std::process::Command::new(cmd).args(args).status()?;
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
    Ok(status)
}
```

Esta função é reutilizável por: SSH, editor externo (`:edit`), `man` interactivo, `less`.
Registar na doc de arquitectura.

#### Passo 1 — Sessão SSH interactiva

- `handle_ssh` em `events/input.rs`: `Enter` sobre host seleccionado chama
  `run_external_interactive("ssh", &[&host.alias])`.
- Após o retorno: `terminal.clear()` + notificação com exit code.
- `AppMode::SshManager` mantém-se inalterado; a suspensão/restauração é transparente.

#### Passo 2 — Histórico de ligações

- Tabela SQLite `ssh_history (id, alias, connected_at, duration_secs, exit_code)`.
- `db.record_ssh_session(alias, duration, exit_code)` chamado após cada sessão.
- Tecla `h` no painel SSH abre popup de histórico (lista estilo Favorites).
- Campo `last_used` em `SshHost` já existe — popular do histórico no `SshManager::new`.

#### Passo 3 — Grupos e labels

- Tabela SQLite `ssh_groups (id, name)` + `ssh_host_groups (alias, group_id)`.
- `SshHost` ganha campo `group: Option<String>`.
- Parser `~/.ssh/config` lê comentários `# group: prod` antes de cada `Host` como label.
- UI: lista agrupada com cabeçalhos de grupo colapsáveis; tecla `G` toggle collapse.
- `n` → Input dialog "Nome do grupo" → cria grupo; `A` adiciona host seleccionado a grupo.

#### Passo 4 — Multi-sessão (tabs)

- `app.ssh_tabs: Vec<SshTab>` onde `SshTab { alias, started_at, status: Pending | Active | Done(ExitStatus) }`.
- Como SSH é bloqueante (suspende TUI), as tabs são historial + fila, não literalmente paralelas.
- UI: barra de tabs no topo do painel SSH com `Ctrl+T` nova sessão, `Ctrl+W` fecha tab.
- Cada tab mostra alias + duração + status colorido.

#### Passo 5 — SFTP (painel de transferência)

- Tecla `f` (files) no host seleccionado abre `AppMode::SftpPanel` (nova variante).
- Layout 2 colunas: local (reusar `FileExplorer`) | remoto (lista via `sftp` subprocess).
- Comandos SFTP via `std::process::Command::new("sftp")` com stdin piped (protocolo batch):
  `ls`, `get`, `put`, `mkdir`, `rm` — **sem biblioteca nativa**, apenas wrapping do binário.
- Progresso de transferência: parsear output `sftp` linha a linha (padrão `TerminalPane`).
- Integração FileManager: tecla `S` sobre ficheiro seleccionado → "Enviar via SFTP" → picker de host.

#### Passo 6 — Tunnels (port-forward)

- `SshTunnel { local_port, remote_host, remote_port, alias, pid }` — estrutura de dados.
- Criar tunnel: `-L local:host:remote` via `tokio::process::Command` em background (não bloqueia TUI).
- Listar tunnels activos no sub-painel (tecla `t` no SSH manager).
- `kill(pid)` para fechar; detectar morte do processo no `tick()`.

#### Critérios de aceitação

- `Enter` sobre um host da config inicia sessão SSH real e restaura a TUI ao sair.
- Histórico mostra as últimas N sessões com duração e exit code.
- SFTP: copiar um ficheiro entre local e remoto sem sair do VOS.
- Multi-tab: registar 3 sessões consecutivas e navegar no histórico por tabs.
- Túnel activo visível no painel; fechar o túnel mata o processo SSH.

---

### 6.6 Chat (IRC primeiro → Matrix depois)

**Referência:** `ROS_Idea/src/md/chat.md` (WhatsApp/Discord/Telegram TUI — layout 2 colunas).
**Objectivo:** chat dentro do VOS. Começar por **IRC** (protocolo trivial, zero deps
pesadas); Matrix (`matrix-sdk`) é decisão à parte por ser enorme.

**Ficheiros (§0.2):** novo `src/modules/chat/mod.rs` + `src/modules/chat/irc.rs`,
`src/ui/chat_panel.rs`; `AppMode::Chat`. Reusa o padrão do side panel animado (`side_pct`)
para a sidebar.

**Crates:** `tokio` (**já**, TcpStream), `tokio-rustls` (**novo**, só p/ IRC+TLS :6697).

**Parser IRC (puro, testável — coração do módulo):**
```rust
#[derive(Debug, PartialEq)]
pub enum IrcMsg {
    Privmsg { from: String, target: String, text: String },
    Join { who: String, chan: String },
    Ping { token: String },
    Other(String),
}
/// formato: [":" prefix SP] command *(SP param) [" :" trailing]
pub fn parse_line(line: &str) -> IrcMsg {
    let (prefix, rest) = match line.strip_prefix(':') {
        Some(s) => { let (p, r) = s.split_once(' ').unwrap_or((s, "")); (Some(p), r) }
        None => (None, line),
    };
    let nick = prefix.map(|p| p.split('!').next().unwrap_or(p).to_string());
    let (cmd, args) = rest.split_once(' ').unwrap_or((rest, ""));
    match cmd {
        "PING" => IrcMsg::Ping { token: args.trim_start_matches(':').to_string() },
        "PRIVMSG" => {
            let (target, text) = args.split_once(" :").unwrap_or((args, ""));
            IrcMsg::Privmsg { from: nick.unwrap_or_default(),
                target: target.to_string(), text: text.to_string() }
        }
        "JOIN" => IrcMsg::Join { who: nick.unwrap_or_default(),
            chan: args.trim_start_matches(':').to_string() },
        _ => IrcMsg::Other(line.to_string()),
    }
}
```
`PING` → responder `PONG :<token>` automaticamente (mantém a ligação viva sem input).
Testes com linhas reais coladas como fixture.

**Rede (não bloquear):** tokio task lê linhas do `TcpStream` → envia `IrcMsg` por `mpsc`
para o `App::tick`; escrita (enviar mensagem) por outro canal → task. Config de
servidores/canais em `settings`/SQLite.

**UI (mockup do `chat.md`):** 2 colunas — sidebar de canais/utilizadores (reusa `side_pct`)
+ pane de mensagens com scroll; **input bar fixa** em baixo (estado próprio, 1 linha — não
o Command mode). Mensagens persistidas em `chat_log (chan, from, text, at)` (cap por canal).

**Notificações:** mensagem com o teu nick e chat sem foco → `Notification::info` global
(sistema existente). Imagens por URL → download → ImageViewer popup (pipeline pronto).

**Fases:** MVP (1 servidor IRC, 1 canal, ler+escrever) → v1.1 multi-canal + sidebar +
persistência → v1.2 notificações + imagens → v2 avaliar Matrix.

**Critérios:** ligar a `irc.libera.chat`, entrar num canal, enviar/receber em tempo real;
`PING/PONG` mantém a ligação sem input.

### 6.7 Video extra (retomar + timeline + export/convert + YouTube)

**Referência:** `ROS_Idea/src/md/video.md` (ffmpeg TUI, youtube-tui; mockup; SQLite).
**Base:** `video/player.rs` (VideoPlayer com Kitty/half-block + prefetch) já existe.
`ffmpeg`/`ffprobe` já são deps de runtime.

**Ficheiros:** estender `src/video/player.rs` + `src/ui/video_panel.rs`; novo
`src/modules/video_jobs.rs` (fila de conversões).

**1. Retomar posição + seek fino:** SQLite `video_positions (path, position_secs,
watched_at)`; gravar no `Esc`/fim, oferecer "retomar em mm:ss" ao abrir. `,`/`.` =
frame-a-frame em pausa.

**2. Timeline com thumbnails:** ao abrir, task em background extrai N=12 thumbnails:
```rust
// ffmpeg -ss <t> -i <path> -frames:v 1 -s 96x54 -f rawvideo -pix_fmt rgb24 -
fn extract_thumb(path: &Path, t_secs: f64) -> Result<Vec<u8>> {
    let out = std::process::Command::new("ffmpeg")
        .args(["-ss", &t_secs.to_string(), "-i", &path.to_string_lossy(),
               "-frames:v", "1", "-s", "96x54", "-f", "rawvideo",
               "-pix_fmt", "rgb24", "-v", "quiet", "-"])
        .output()?;
    Ok(out.stdout)   // 96*54*3 bytes RGB24
}
```
Render como faixa de half-blocks por baixo da barra de progresso; tecla numérica/click
faz seek. IDs Kitty reservados para a faixa (documentar em `kitty.rs`).

**3. Export/convert (ffmpeg TUI):** `e` abre form (formato mp4/webm/gif · resolução ·
corte in/out hh:mm:ss · crf). Montar argv e correr em `tokio::process`, **parseando o
progresso** do stderr do ffmpeg:
```rust
/// ffmpeg escreve "time=00:01:23.45" no stderr; ÷ duração total = progresso.
fn parse_ffmpeg_time(line: &str) -> Option<f64> {
    let t = line.split("time=").nth(1)?.split_whitespace().next()?;   // "00:01:23.45"
    let mut p = t.split(':');
    let h: f64 = p.next()?.parse().ok()?;
    let m: f64 = p.next()?.parse().ok()?;
    let s: f64 = p.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}
```
Fila de conversões: `VecDeque` de jobs, 1 a correr de cada vez, estado em
`video_jobs (id, input, args_json, status, created_at)` (sobrevive a restart como
"interrompido").

**4. YouTube (v2):** `y` → pesquisa via `yt-dlp --dump-json <query>` (JSON por linha),
grelha com thumbnails (estilo youtube-tui); tocar = stream no VideoPlayer interno ou
delegar a `mpv`. `yt-dlp` detectado como as apps da Fase 4.

**Fases:** MVP (retomar + `,`/`.`) → v1.1 timeline thumbnails → v1.2 export/convert +
fila → v2 YouTube.

**Critérios:** reabrir um vídeo oferece retomar; timeline navegável; converter mp4→gif
mostra progresso real e notifica ao acabar sem bloquear.

---

### 6.9 IDE / Code — integrar o RIDE (LSP · DAP · VCS)

**Referência:** `ROS_Idea/src/IDE/RIDE.MP4` + projecto irmão `rust_apps/RIDE/`
(MVVM: `model/{buffer,editor,syntax,lsp,dap,vcs,terminal,plugin_host}`, `view`, `viewmodel`).
**Objectivo:** o VOS tem um editor (syntect) para edições rápidas; falta um ambiente de
desenvolvimento (diagnósticos LSP, debug DAP, git inline). **Não reimplementar** — reutilizar o RIDE.

**Decisão de arquitectura:** duas fases, da mais barata para a mais integrada.

**v1 — lançar o RIDE como app externa (barato, imediato):** reusa
`run_external_interactive` (§6.8 passo 0), o mesmo mecanismo do SSH:
```rust
// FileManager 'E' (ou launcher) sobre uma pasta/ficheiro de código:
let target = app.explorer.selected_path();
run_external_interactive("ride", &[&target.to_string_lossy()], app.kbd_enhanced)?;
app.force_full_redraw = true;   // limpar artefactos ao voltar (mesmo padrão do SSH)
```
Detectar o binário `ride` no PATH (ou `../RIDE/target/release/ride` em dev); fallback =
editor interno. Registar `code`/`ide` no launcher e `:code <path>`.

**v2 — cliente LSP mínimo dentro do editor do VOS:** portar/reutilizar `model/lsp.rs` do
RIDE. LSP é JSON-RPC sobre stdio de um language server (`rust-analyzer`, `pyright`…):
```rust
pub struct LspClient { child: tokio::process::Child /* stdin/stdout piped */ }
impl LspClient {
    // 1) spawn do server  2) handshake initialize/initialized
    // 3) notificar didOpen/didChange ao editar
    // 4) receber textDocument/publishDiagnostics → sublinhados no buffer
    // 5) pedir completion/definition/hover sob demanda
}
```
Enquadramento de mensagens: header `Content-Length: N\r\n\r\n` + payload JSON
(`serde_json`, já é dep). Task tokio lê respostas → `mpsc` → `App::tick`; diagnósticos
pintam-se como spans no editor existente (reusa o modelo de spans do syntect). DAP (debug)
e plugin_host ficam para v3 — o RIDE externo (v1) já os cobre entretanto.

**Ficheiros:** v1 — handler no FileManager/launcher + detecção de binário. v2 — novo
`src/editor/lsp.rs` (porta do RIDE) + campos no editor + render de diagnósticos.

**Crates:** nenhum novo para v1. v2: `serde_json` (**já**); o `rust-analyzer` é dep de
runtime detectada, não uma crate.

**Critérios:** v1 — `E` num projecto abre o RIDE e volta limpo ao VOS. v2 — abrir um `.rs`
mostra diagnósticos do rust-analyzer sublinhados; autocompletar responde.

---

### 6.10 Customização dinâmica (pywal / hellwal)

**Referência:** `customize-hellwal-colors.gif`, `customize-textfox-tui-theme.png`,
`customize-btop-options.png`, `customize-hyprdots-themeswitcher.webp` (nós "custumeze"/"setting look").
**Objectivo:** o theme switcher (3.2) tem 5 temas fixos; falta **gerar** a paleta a partir
de uma imagem (wallpaper) e aplicá-la ao tema/terminal/apps — o padrão pywal/hellwal.

**Ficheiros (§0.2):** novo `src/ui/theme_gen.rs` (extractor) + estender `src/ui/theme.rs`
(`Theme` runtime) e o theme switcher; `AppMode::ThemeSwitcher` já existe.

**Crates:** `image` (**já é dep** — já decodifica png/jpg/webp). Sem dependência nova: o
k-means são ~40 linhas.

**Extracção de paleta (k-means sobre os pixéis):**
```rust
/// Reduz a imagem a K cores dominantes; devolve-as ordenadas por luminância.
pub fn extract_palette(img: &image::RgbImage, k: usize, step: usize) -> Vec<[u8; 3]> {
    let px: Vec<[f32; 3]> = img.pixels().step_by(step)          // ~4000 amostras
        .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
    let mut cent = seed_kmeans(&px, k);                         // centróides espaçados
    for _ in 0..10 {
        let mut sum = vec![[0f32; 3]; k];
        let mut cnt = vec![0u32; k];
        for p in &px {                                          // atribuir
            let c = nearest(&cent, p);
            for j in 0..3 { sum[c][j] += p[j]; }
            cnt[c] += 1;
        }
        for i in 0..k {                                         // recalcular
            if cnt[i] > 0 { for j in 0..3 { cent[i][j] = sum[i][j] / cnt[i] as f32; } }
        }
    }
    let mut out: Vec<[u8; 3]> = cent.iter()
        .map(|c| [c[0] as u8, c[1] as u8, c[2] as u8]).collect();
    out.sort_by_key(|c| luminance(*c));                         // dark → light
    out
}
```
Mapear as K cores para os *slots* do `Theme` (bg = mais escura, fg = mais clara, accent =
mais saturada…), garantir contraste mínimo (subir/descer luminância até ≥4.5:1 — reusa a
regra de acessibilidade da 5.2) e aplicar em runtime com **preview ao vivo** (o render já
lê `app.theme()`).

**Wallpaper picker + exportação:** escolher a imagem pelo FileManager (`w` = set wallpaper);
guardar em `settings.wallpaper`. Exportar a paleta para `~/.cache/vos/colors.sh`
(`export COLOR0=#rrggbb …`) e `colors.Xresources`, para o resto do ambiente do utilizador
herdar (o que o pywal faz). Persistir o tema gerado em `data/themes/<nome>.toml`.

**"Setting look" (galeria):** grelha de temas (fixos + gerados) com preview; ligada ao
Control Center (§7.4, categoria Aparência) e ao wallpaper/backdrop (§7.8).

**Critérios:** escolher um wallpaper gera um tema coerente e legível (contraste ok)
aplicado ao vivo; a paleta exportada existe em `~/.cache/vos/`; o tema sobrevive a restart.

---

### 6.11 USB / Dispositivos externos (montar · transferir · ejectar)

**Referência:** nós "Flash USB / Dispositivos externos" do canvas (ideia nova, 2026-07-09) +
`ROS_Idea/src/usb devices/` (`usb-mmtui-mount-manager.gif`, `transfer-ppcp-copy-progress.gif`,
`transfer-superfile-footer-progress.gif`). Refs: mmtui (mount manager Rust sobre udisks2),
rmount, mount.yazi, cpui/ppcp (cópia com velocidade em tempo real), superfile (progresso no
footer), task manager async do Yazi (cancelamento + prioridades).
**Objectivo:** painel de dispositivos ligados (pens USB, discos externos): montar/desmontar
sem root, copiar/mover/apagar **PC ⇄ flash** em dual-pane, e ver cada transferência com
barra de progresso + **gráfico de velocidade a crescer** (MB/s) + ETA. Ejecção segura.

**Ficheiros (§0.2):** novo `src/modules/devices.rs` (enumerar + montar/desmontar/ejectar +
hot-plug), novo `src/fs/copy_engine.rs` (motor de cópia com progresso — desenhado para ser
reutilizado pelo FileManager e pelo Transfer §6.3), novo `src/ui/devices_panel.rs`;
`AppMode::DevicePanel`.

**Crates:** nenhum obrigatório na v1 — envolver binários do sistema (princípio "zero
daemons"): Linux `lsblk -J` (enumerar, JSON) + `udisksctl mount/unmount/power-off`;
macOS `diskutil list -plist external physical` + `diskutil mount/unmount/eject`.
v2 (eventos em vez de polling): `zbus`/`udisks2` (sinais DBus `InterfacesAdded/Removed`,
Linux) e DiskArbitration via FFI (macOS) — só se o polling de 2 s se mostrar insuficiente.

**Enumeração (poll em thread de fundo + mpsc, padrão do Weather 6.5):**
```rust
pub struct ExtDevice {
    pub dev: String,                    // /dev/sdb1 · /dev/disk4s1
    pub label: String, pub fs: String,
    pub size: u64, pub used: Option<u64>,
    pub mount_point: Option<PathBuf>,   // None = desmontado
}
// Linux: `lsblk -J -o NAME,LABEL,FSTYPE,SIZE,MOUNTPOINT,RM,TYPE` → filtrar RM=true
// macOS: `diskutil list -plist external physical` + `diskutil info -plist <dev>`
// Diff entre polls → NotificationKind::Info "Pen 'X' ligada" / "removida"
```

**Motor de cópia com progresso (blocos de 1 MiB, cancelável):**
```rust
pub enum OpKind { Copy, Move, Delete }
pub enum OpState { Pending, Active, Done, Error(String), Cancelled }
pub struct FileOp { pub kind: OpKind, pub src: PathBuf, pub dst: PathBuf,
                    pub bytes_done: u64, pub bytes_total: u64, pub state: OpState }

// Uma task tokio por operação; cancelamento por Arc<AtomicBool> (sem dep nova).
async fn copy_with_progress(src: &Path, dst: &Path, tx: mpsc::Sender<(u64, u64)>,
                            cancel: Arc<AtomicBool>) -> io::Result<()> {
    let total = tokio::fs::metadata(src).await?.len();
    let mut r = tokio::fs::File::open(src).await?;
    let mut w = tokio::fs::File::create(dst).await?;
    let mut buf = vec![0u8; 1024 * 1024];               // ~1 MiB por bloco
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {             // cancelado → apagar dst parcial
            drop(w); let _ = tokio::fs::remove_file(dst).await;
            return Err(io::Error::other("cancelled"));
        }
        let n = r.read(&mut buf).await?;
        if n == 0 { break; }
        w.write_all(&buf[..n]).await?;
        done += n as u64;
        let _ = tx.try_send((done, total));             // UI: try_send, nunca bloquear
    }
    w.sync_all().await?;                                // flush real antes de "Done"
    Ok(())
}
```
Velocidade = média móvel das amostras (Δbytes/Δt) num `VecDeque` (~30 amostras) → alimenta
um `Sparkline` (o mesmo padrão do rx/tx do Network Panel 2.3); ETA = restante ÷ média.
Mover = copiar + apagar origem (entre devices) ou `rename` (mesmo fs); apagar com
`ConfirmAction` como no FileManager.

**UX do painel:**
- Lista: label, fs, capacidade, usado/livre (gauge fino), estado ●/○ montado/desmontado.
- `Enter` = montar (se preciso) e abrir **dual-pane** PC ⇄ flash: dois exploradores lado a
  lado (reusa `fs/explorer.rs`), `Tab` troca o lado, `c`/`m`/`Del` copia/move/apaga para o
  outro lado.
- Zona de transferências em baixo (estilo footer do superfile): uma linha por operação com
  gauge + MB/s + sparkline + ETA; `x` cancela a operação seleccionada.
- `e` = ejectar seguro: sync + unmount + power-off/eject, com `ConfirmAction`; bloqueado
  enquanto houver operações activas nesse device.
- Hot-plug: pen inserida/removida aparece/some sozinha; device removido com operações
  activas → operações marcadas `Error` + notificação de erro.

**SQLite (§0.4):** nada obrigatório na v1 (operações são efémeras); opcional registar no
`transfer_jobs` do §6.3 para histórico unificado de transferências.

**Fases:** MVP (listar + montar/desmontar/ejectar + notificação hot-plug) → v1.1 dual-pane
+ copy engine com barra/velocidade/ETA → v1.2 fila multi-operação cancelável + mover/apagar
→ v2 eventos DBus/DiskArbitration nativos (sem polling).

**Critérios:** ligar uma pen fá-la aparecer na lista em ≤2 s; `Enter` monta e abre o
dual-pane; copiar um ficheiro grande mostra barra + gráfico de MB/s a crescer + ETA sem
nunca bloquear a UI; `x` cancela e remove o destino parcial; `e` ejecta com segurança;
funciona em Linux e macOS (degradação: sem `udisksctl`/`diskutil` → painel read-only com aviso).

---

### 6.12 Autocomplete no terminal (fish-style)

**Referência:** nós "Autocomplete no terminal VOS" do canvas (ideia nova, 2026-07-09) +
`ROS_Idea/src/autocomplete/` (`autocomplete-fish-autosuggestion.webp`,
`autocomplete-inshellisense-popup.gif`, `autocomplete-carapace-completions.png`).
Refs: fish (autosuggestions + menu — o modelo a seguir), carapace (specs de flags
multi-shell), inshellisense (popup estilo IDE), reedline (`ColumnarMenu`/`IdeMenu` em Rust),
rustyline (hints à direita do cursor), nucleo (fuzzy matcher do helix).
**Objectivo:** no terminal interno, ao digitar o 1º caractere aparece uma lista por baixo
do cursor; cada tecla **refiltra (fuzzy)**; `↑/↓` selecciona; `Tab`/`Enter` completa a
palavra; `Esc` fecha. Extra: **ghost text** inline (estilo fish) com a melhor sugestão.
Nota: o terminal interno é *line-based* (o VOS é dono da linha de input — §CLAUDE.md), por
isso o popup não conflitua com nenhuma shell: tudo acontece antes de enviar a linha ao processo.

**Ficheiros (§0.2):** novo `src/terminal/autocomplete.rs` (fontes + matcher + estado do
popup); overlay no render em `src/ui/terminal_panel.rs` (desenhado DEPOIS do painel, como
os popups existentes); interceptar teclas no handler do terminal em `src/events/input.rs`
antes de escrever na linha. **Sem AppMode novo** — é um overlay do modo Terminal.

**Crates:** nenhum na v1 — matcher próprio (~40 linhas testáveis). v2: `nucleo` se a
qualidade do fuzzy ficar aquém com listas grandes.

**Fontes de sugestões (por prioridade):**
```rust
pub enum SuggestionSource { History, PathBin, CwdFile, Flag }
pub struct Suggestion { pub text: String, pub source: SuggestionSource, pub score: i64 }
```
1. **Histórico** — `command_history` do `app.db` (já existe), mais recente primeiro.
2. **Binários do `$PATH`** — scan 1× no arranque em thread de fundo → cache `Vec<String>`
   (re-scan com `F5`); usados só na 1ª palavra da linha.
3. **Ficheiros/dirs do cwd** — para os argumentos (2ª palavra em diante); respeita o cwd
   do terminal interno.
4. **Flags/subcomandos (v2)** — `carapace <cmd> export` (JSON) se o binário `carapace`
   existir no PATH (detecção como as apps da Fase 4); cache por comando.

**Estado + popup:**
```rust
pub struct Autocomplete {
    pub visible: bool,
    pub items: Vec<Suggestion>,   // filtrados+ordenados; render máx. 8 com scroll
    pub selected: usize,
    pub prefix: String,           // palavra sob o cursor (extractor com testes)
}
```
- Overlay ratatui ancorado à posição do cursor (linha de input do terminal): abre para
  baixo; se não houver espaço, abre para cima. Nunca tapa a própria linha de input.
- Ghost text: resto da melhor sugestão em `DarkGray` inline após o cursor; `→`/`End`
  aceita o ghost; `Tab`/`Enter` aceita o item seleccionado da lista.
- `Esc` fecha só o popup (o 2º `Esc` é que sai do modo — não roubar o Esc habitual).
- Highlight dos caracteres que fizeram match (spans bold/accent do tema).

**Matcher v1 (sem dep):** prefixo (score alto) > subsequência (médio; bónus por
consecutivos e por início de palavra) > contains (baixo); empate → `History` > `PathBin` >
`CwdFile`, depois o mais curto primeiro. Refiltrar a cada tecla sobre o cache é ≤ alguns ms
para milhares de entradas — se um dia não chegar, trocar por `nucleo` sem mexer na UI.

**Fases:** MVP (histórico + `$PATH` + cwd, popup + Tab/Enter/Esc) → v1.1 ghost text +
highlight de match → v2 specs do carapace (flags/subcomandos).

**Critérios:** digitar `g` abre a lista (git, grep, …) por baixo do cursor; continuar a
digitar refiltra sem flicker; `↑/↓`+`Tab` completa a palavra; ghost text aparece para a
melhor sugestão do histórico e `→` aceita-o; `Esc` fecha; o scan do `$PATH` nunca bloqueia
o arranque nem o event loop; testes unitários do matcher e do extractor de palavra sob o cursor.

---

## Fase 7 — Ambiente de Desktop completo (DE no terminal)

> A casca que está **sempre presente** e cola as apps num OS. Onde as Fases 2–6 são
> "abrir uma app", a Fase 7 é "o sítio onde as apps vivem": dock, launcher, workspaces,
> notificações, energia. Regras transversais: teclado-first, tudo por cima do WM/estado
> que já existe, nada bloqueia o loop, degradação graciosa.

### 7.1 Dock / Taskbar persistente

**Objectivo:** barra fina sempre visível (1–2 linhas) no fundo (ou topo): apps abertas
(janelas/tabs do WM), favoritos fixos, relógio e uma *tray* de estado (rede/som/bateria/
notificações). É o "painel" do KDE/GNOME.

**Ficheiros:** novo `src/ui/dock.rs` (render puro); estado em `App` (`dock_visible: bool`,
`dock_pinned: Vec<AppMode>`); sem `AppMode` próprio (é reserva de 1 linha no layout raiz).

**Passos:**
1. Reservar 1 linha no layout raiz (`main.rs render_main`) quando `dock_visible` — como a
   status bar já faz; a status bar pode fundir-se com o dock ou ficar por cima dele.
2. Conteúdo: à esquerda os *pinned* (`ICON_MODE_*` de `ui/icons.rs`), ao centro as janelas
   abertas do WM (`app.wm` já tem a árvore — listar folhas com título+modo), à direita a
   tray (§7.9) + relógio (`chrono`).
3. Interacção: `Super` toggla; `Super`+número salta para a n-ésima app; clique (mouse 3.4)
   idem. App focada destacada com o accent do tema.
4. Persistir `dock_pinned` em `settings`.
```rust
// esboço do render (chamado no fim de render_main quando app.dock_visible):
pub fn render_dock(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Length(20),      // pinned
        Constraint::Min(0),          // janelas abertas
        Constraint::Length(24),      // tray + relógio
    ]).split(area);
    // pinned:  app.dock_pinned.iter().map(icon_for_mode)
    // janelas: app.wm.leaves().map(|w| format!("{} {}", icon_for_mode(w.mode), w.title))
    // tray:    net/som/bateria (§7.9) + Local::now().format("%H:%M")
}
```

**Critérios:** o dock mostra as apps abertas e actualiza o relógio; `Super`+n salta;
esconder/mostrar é instantâneo e não estraga o layout das apps.

### 7.2 Workspaces (desktops virtuais)

**Objectivo:** N workspaces (default 9), cada um com o seu layout; `Super+1..9` salta,
`Super+Shift+1..9` move a janela focada. Indicador no dock.

**Ficheiros:** estender `src/wm/mod.rs` (o WM já tem a árvore de janelas); `App` ganha
`workspaces: Vec<WmTree>` + `current_ws: usize`.

**Passos:**
1. Transformar o `wm` único num `Vec` de árvores; o render usa `workspaces[current_ws]`.
2. `switch_ws(i)`: guarda a árvore actual, carrega a `i` (foco preservado por workspace).
3. `move_window_to_ws(i)`: retira a folha focada da árvore actual e insere na `i`.
4. Keybinds no `KeybindEngine` (`core/keybinds.rs`): `Super+1..9`, `Super+Shift+1..9`.
   Indicador `[1 2•3 …]` no dock (§7.1) e/ou status bar.

**Critérios:** abrir apps em WS diferentes e saltar entre elas preserva os layouts; mover
uma janela para outro WS fá-la desaparecer do actual e aparecer no destino.

### 7.3 Global Launcher (Spotlight / KRunner)

**Objectivo:** um campo único (evolução do Command Palette) que pesquisa **tudo**: apps
(modos), ficheiros (recentes/favoritos/notas), definições, cálculo inline e acções "ao
vivo" (abrir URL, converter unidades). `Super+Space`.

**Ficheiros:** estender `src/ui/command_palette.rs` (já tem fuzzy + itens dinâmicos da 6.1).

**Passos:**
1. *Providers* — um trait que devolve resultados pontuados para a query:
```rust
pub trait LaunchProvider { fn query(&self, q: &str) -> Vec<LaunchItem>; }
pub struct LaunchItem {
    pub icon: &'static str, pub label: String, pub detail: String,
    pub action: PaletteAction, pub score: i64,
}
```
2. Providers concretos: `AppsProvider` (modos — já existe), `FilesProvider` (recentes/
   favoritos/notas do SQLite), `SettingsProvider` (campos do Config), `CalcProvider` (se a
   query avalia por `modules::calc::eval` → resultado como 1º item, `Enter` copia),
   `WebProvider` (`http…`/`?…` → abrir no Browser 6.2).
3. Agregar+ordenar por `score` (fuzzy-matcher já é dep) com secções por provider.
4. `Super+Space` abre; `:` continua a ser o command mode clássico.

**Critérios:** escrever "2+2" mostra 4; "conf" mostra a app Config; um ficheiro recente
aparece; `http://…` oferece abrir no browser — tudo no mesmo campo.

### 7.4 Control Center (Definições unificadas)

**Objectivo:** um só ecrã master-detail com categorias — reaproveita painéis existentes em
vez de duplicar. Como o "Definições" do GNOME.

**Ficheiros:** novo `src/ui/control_center.rs`; `AppMode::ControlCenter`.

**Passos:**
1. Coluna esquerda com categorias (Aparência · Rede · Som · Energia · Data/Hora ·
   Utilizadores · Dispositivos · Atalhos); direita renderiza o painel da categoria.
2. Reusar: Aparência → theme switcher + wallpaper (3.2/6.10); Rede → Network Panel (2.3);
   Som/Energia/Dispositivos → device control (7.9); Data/Hora → tz+`chrono`; Atalhos →
   help/keymap (3.5); Utilizadores → `whoami`/`id`/`who`.
3. Entrada: palette "Settings" e `:settings` passam a abrir o Control Center (o Config
   simples fica como a categoria "Geral").

**Critérios:** navegar categorias mostra cada painel; mudar tema/volume aqui reflecte-se na
app; sem duplicar a lógica dos painéis reutilizados.

### 7.5 Power menu + Lock screen

**Objectivo:** terminar sessão como num OS — Shutdown/Reboot/Logout/Suspend/Lock — e um ecrã
de bloqueio.

**Ficheiros:** novo `src/ui/power.rs` (overlay) + `src/ui/lock.rs`; `AppMode::Lock`; power
menu é um `DialogKind`/overlay.

**Passos:**
1. Power menu (`F10` ou launcher): lista com confirmação. Comandos por OS:
```rust
fn power_cmd(action: PowerAction) -> (&'static str, Vec<&'static str>) {
    match (action, cfg!(target_os = "macos")) {
        (PowerAction::Shutdown, true)  => ("osascript", vec!["-e", "tell app \"System Events\" to shut down"]),
        (PowerAction::Shutdown, false) => ("systemctl", vec!["poweroff"]),
        (PowerAction::Reboot,   false) => ("systemctl", vec!["reboot"]),
        (PowerAction::Suspend,  false) => ("systemctl", vec!["suspend"]),
        (PowerAction::Suspend,  true)  => ("pmset", vec!["sleepnow"]),
        (PowerAction::Logout,   _)     => ("loginctl", vec!["terminate-user", "$USER"]),
        _ => ("true", vec![]),
    }   // SEMPRE atrás de ConfirmAction; nunca elevar privilégios silenciosamente
}
```
2. Lock screen (`AppMode::Lock`): overlay opaco com relógio grande + campo de password;
   guardar **hash** (`argon2`/sha256+salt) em `settings`/ficheiro, **nunca** a password do
   sistema; enquanto bloqueado, o event loop só aceita o campo de password. Auto-lock por
   inactividade opcional (`settings.lock_timeout`).

**Critérios:** o power menu pede confirmação e executa o comando certo por OS; o lock
esconde o conteúdo e só destranca com a password correcta; `Esc` não fura o lock.

### 7.6 Notification Center

**Objectivo:** as notificações já existem como toasts efémeros; falta **histórico** + "não
incomodar" + painel reabrível.

**Ficheiros:** estender o sistema de notificações do `App`; novo `src/ui/notif_center.rs`
(overlay/side panel).

**Passos:**
1. Persistir cada notificação em memória (`VecDeque`, cap ~200) e opcionalmente em
   `notifications (id, level, text, at, read)` no SQLite.
2. Painel (tecla no dock/tray ou `Super+N`): lista agrupada por app/nível, marcar lida,
   limpar tudo.
3. "Não incomodar": flag em `App`; toasts suprimidos mas na mesma guardados no histórico;
   indicador na tray.

**Critérios:** disparar 3 notificações e reabri-las no centro; DND suprime o toast mas o
histórico regista-o.

### 7.7 Trash / Reciclagem

**Objectivo:** apagar no FileManager passa a **mover para o lixo** (restaurável), não `rm`.

**Ficheiros:** novo `src/fs/trash.rs`; ligar ao `handle_file_manager`.

**Passos:**
1. Spec freedesktop: mover para `~/.local/share/Trash/files/` + escrever
   `~/.local/share/Trash/info/<nome>.trashinfo`. macOS: `~/.Trash`. Colisões → sufixo numérico.
```rust
pub fn trash(path: &Path) -> Result<()> {
    let (files, info) = trash_dirs()?;                     // cria on-demand
    let name = unique_name(&files, path.file_name().unwrap());
    let body = format!("[Trash Info]\nPath={}\nDeletionDate={}\n",
        path.display(), chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"));
    std::fs::write(info.join(format!("{name}.trashinfo")), body)?;
    std::fs::rename(path, files.join(name))?;              // rename = instantâneo no mesmo FS
    Ok(())
}   // fallback cross-FS: copiar + apagar
```
2. FileManager: `Del`/`d` → `trash()`; **`Shift+Del` = `rm` real** (com ConfirmAction). Novo
   painel "Lixo" para ver/restaurar/esvaziar (restaurar = ler o `.trashinfo` e `rename` de volta).

**Critérios:** apagar manda para o lixo e o ficheiro reaparece ao restaurar; esvaziar pede
confirmação; `Shift+Del` apaga mesmo.

### 7.8 Wallpaper / Backdrop

**Objectivo:** fundo do ambiente (arte ASCII animada ou imagem via Kitty) por trás dos
painéis; base visual do tema dinâmico (6.10).

**Ficheiros:** novo `src/ui/backdrop.rs`; render **antes** de tudo em `render_main`.

**Passos:**
1. Modo ASCII: padrões (ondas, matrix rain, gradiente do tema) em `Color` dim por baixo, com
   os painéis por cima (a maioria é opaca; o Menu/dashboard pode ser translúcido).
2. Modo imagem (Kitty): desenhar `settings.wallpaper` uma vez com `image_id` reservado, por
   baixo dos painéis (técnica do ImageViewer, mas em background e sem foco). Fora do Kitty,
   degradar para ASCII.
3. Ligado ao 6.10: a imagem do wallpaper alimenta o extractor de paleta.

**Critérios:** no Menu vê-se o backdrop; abrir uma app opaca cobre-o; sem Kitty há fallback
ASCII; sem custo de CPU percetível quando estático.

### 7.9 Device control (áudio · bluetooth · brilho · bateria)

**Objectivo:** controlos de sistema que um DE tem na tray: volume/saída de áudio, Bluetooth,
brilho e bateria.

**Ficheiros:** novo `src/modules/devices.rs`; expõe getters/acções para a tray (7.1) e o
Control Center (7.4).

**Passos (envolver binários, §0.3, com detecção + fallback):**
- **Áudio:** Linux `pactl`/`wpctl` (`get-volume`, `set-volume`, `set-sink`), macOS
  `osascript`/`SwitchAudioSource`. Volume na tray, `+`/`-` ajusta.
- **Bluetooth:** `bluetoothctl` (Linux) / `blueutil` (macOS): listar/emparelhar/ligar.
- **Brilho:** `brightnessctl` (Linux) / `pmset`/`brightness` (macOS).
- **Bateria:** `pmset -g batt` (macOS) / `upower -i` (Linux) → percentagem + a-carregar:
```rust
pub struct Battery { pub pct: u8, pub charging: bool }
pub fn battery() -> Option<Battery> {
    // macOS: pmset -g batt  →  "...  87%; discharging; ..."
    let out = std::process::Command::new("pmset").args(["-g", "batt"]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let pct = s.split('%').next()?
        .rsplit(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?;
    let charging = s.contains("; charging") || s.contains("AC Power");
    Some(Battery { pct, charging })
}
```

**Critérios:** a tray mostra volume e bateria reais; ajustar o volume no VOS muda o volume
do sistema; onde o binário não existe, o controlo aparece desactivado (não crasha).

### 7.10 Quick Settings ("system poppout")

**Objectivo:** painel de acesso rápido (estilo quick settings do GNOME/Android): toggles de
tema, wi-fi, som, DND, brilho — o nó "system poppout"/"system navigation" do canvas.

**Ficheiros:** novo `src/ui/quick_settings.rs`; poppout (reusa a animação `side_pct` ou um
canto). Acessível pela tray do dock (7.1) ou `Super+Q`.

**Passos:** grelha de *toggles* grandes (2×N): tema dark/light, Wi-Fi on/off (via device/net),
volume (slider), DND (7.6), brilho (7.9), captura de ecrã. Cada toggle chama a acção do
módulo respectivo. É sobretudo *cola* sobre 7.4/7.6/7.9.

**Critérios:** abrir o poppout e alternar o tema/DND reflecte-se imediatamente; fecha com
Esc/clique fora.

### 7.11 Onboarding / first-run

**Objectivo:** primeira execução acolhedora — tema, pastas (notas/música), detecção de apps
e tour de atalhos.

**Ficheiros:** novo `src/ui/onboarding.rs`; `AppMode::Onboarding`; flag `settings.onboarded: bool`.

**Passos:** wizard de 3–4 páginas (tema → pastas → detecção de deps `ffmpeg/docker/gh/yt-dlp/
rust-analyzer` com ✓/✗ → atalhos essenciais). No fim grava `onboarded = true`. A detecção
reusa o helper `which`-like já usado pelos módulos da Fase 4.

**Critérios:** 1ª execução mostra o wizard; execuções seguintes vão direto ao dashboard; a
detecção lista correctamente o que está/não está instalado.

---

## Fase 8 — Distribuição

1. **Config location** (§7.3 das Questões): migrar para `~/.config/vos/config.toml` +
   `~/.local/share/vos/app.db` (crate `dirs`), com fallback dev (`./config`) e migração
   automática na 1ª execução. Fazer **antes** de qualquer feature nova que persista caminhos.
2. **install.sh**: detectar deps opcionais (ffmpeg, docker, gh, yt-dlp, pactl…) e avisar
   claramente o que falta e para que serve.
3. **Self-update** `vos --update`: última release do GitHub → binário da plataforma →
   substituir-se (com backup).
4. **Docs (5.4)** + `README` com asciinema/gif.
5. **Empacotamento**: Homebrew tap (macOS), AUR (Arch), `.deb`/`.rpm`.

---

## §7 — Questões em aberto: decisões propostas

1. **Plugin API dinâmico** — manter built-in compilado (decisão actual correcta).
   Re-avaliar só se surgir necessidade real; alternativa futura: plugins por
   subprocess com protocolo JSON, nunca `.so`.
2. **Windows** — não planear. Guard de compilação onde for preciso e nada mais.
3. **Config location** — migrar para `~/.config/vos/config.toml` e
   `~/.local/share/vos/app.db` (crate `dirs`), **com fallback**: se
   `config/config.toml` relativo existir, usá-lo (modo dev). Fazer isto **antes**
   da 3.7 (Config TUI) para não persistir no sítio errado. Migração automática:
   primeira execução copia os ficheiros antigos se existirem.
4. **Async git** — manter binário `git` (já há 20+ funções a funcionar em
   `plugins/git.rs`; git2 não compensa o peso). Para operações lentas
   (push/pull), mover para `tokio::process` com notificação "a correr…" — hoje
   bloqueiam o loop.
5. **Multiplexer** — VOS *é* o multiplexer (tabs+splits próprios); não integrar
   tmux. Garantir apenas que corre bem *dentro* de tmux (degradação: sem Kitty
   graphics — a detecção actual via env-var já trata disso).

---

## Ordem de execução recomendada

Dependências entre tarefas (implementar pela ordem dentro de cada linha):

1. **§7.3 config location** → 3.7 Config TUI → 3.1 hot-reload
2. **3.2 Theme system** → 3.3 SysMon visual → 5.2 acessibilidade
3. **2.1 Log Viewer** (desenhar `LogSource` extensível) → 4.3 Docker logs
4. **2.3 Network Panel** (`local_ip()`) → 6.3 Transfer
5. **3.5 help contextual / rodapés** cedo — cada módulo novo já nasce com hints
6. 2.4 Git Workspace e 2.5 Disk Manager são independentes — bons para paralelizar
7. Fase 6: Notes (6.1) primeiro — máximo reuso, valida o palette dinâmico que
   o Browser (6.2) também usa
8. **§7.3 config location (Fase 8)** antes de features novas que persistam caminhos
   (wallpaper, temas gerados, jobs de transferência)
9. **6.2 Browser** primeiro entre as apps novas — o `WebProvider` do Launcher (7.3)
   e o download do Transfer (6.3) reutilizam o seu fetch
10. **6.10 Customização** depende do 3.2 (Theme) e alimenta 7.8 (wallpaper/backdrop)
11. Fase 7 (DE): **7.1 Dock → 7.2 Workspaces → 7.10 Quick Settings** nesta ordem — o
    dock hospeda a tray e o indicador de workspaces; o poppout é cola sobre 7.4/7.6/7.9
12. **6.9 IDE v1** (lançar RIDE) é barato e independente — bom candidato a paralelizar
13. **6.11 USB/Devices** partilha o motor de progresso com o 6.3 Transfer — desenhar o
    `copy_engine` (blocos + mpsc + cancelamento) uma vez para os dois (e para o
    FileManager); o sparkline de MB/s reusa o padrão rx/tx do 2.3
14. **6.12 Autocomplete** só toca no terminal interno — independente, bom para
    paralelizar; o cache de binários do `$PATH` serve depois o Launcher (7.3)

**Regras transversais para o agente implementador:**
- Uma tarefa = um conjunto coeso de commits; `cargo check && cargo clippy && cargo fmt`
  antes de cada commit; correr sempre da raiz do repo.
- Nunca bloquear o event loop (>16ms) — qualquer I/O incerto vai para tokio/thread.
- Toda a acção destrutiva passa por `ConfirmAction` + `handle_dialog`.
- Erros de processos externos → `Notification::error` com o stderr, nunca panic.
- Actualizar o checkbox correspondente no `Doc/ROADMAP.md` ao concluir.
