# Nitor — правила для Claude

Програма для Windows: керування RGB-прожекторами через uDMX та інші DMX-інтерфейси.
Rust (Cargo workspace), інтерфейс на egui/eframe (рушій малювання wgpu). Один `nitor.exe` без DLL.
Користувачі й документація — українською.

## Структура

- `crates/nitor-core` — рушій без інтерфейсу: `engine.rs` (стан, команди, undo/redo, сцени, рендер), `fixtures.rs`
  (профілі, рендер приладу), `effects.rs`, `scene_file.rs` (`.nitor`), `config.rs` (`config.json` версії 2),
  `library.rs` (вбудована бібліотека моделей), `userdata.rs` (вибіркове видалення даних), `outputs/` (uDMX через rusb,
  Enttec/Open DMX через serialport, Art-Net, sACN, симулятор; потік відправки з паузою на порожньому кадрі).
- `crates/nitor` — програма: `app.rs` (стан інтерфейсу, дії `Act`), `ui_main.rs` (головне вікно), `dialogs.rs` (вікна),
  `menus.rs` (контекстні меню), `hotkeys.rs`, `splash.rs` (вікно запуску), `update.rs` (GitHub Releases),
  `core.rs` (потік рушія 40 кадрів/с), `winshell.rs` (одна копія, події `Local\Nitor.Show` / `Local\Nitor.Quit`),
  `already.rs` (попередження другої копії; знімок — `NITOR_SCREENSHOT_ALREADY=1`).
- `crates/nitor-ui` — дизайн-система: `theme.rs` (токени), `widgets.rs` (кнопки, повзунки, поля), `icons.rs`.
- `crates/nitor-setup` — `Nitor-Setup.exe` (встановлення, видалення, `/S`, `/update --wait-pid`); програма вкладена
  стисненою (`NITOR_APP_EXE` під час збирання). Копія інсталятора в теці програми — `Uninstall.exe`.
- `crates/nitor-library` — збирає `crates/nitor-core/assets/library.json.gz` з клонів OFL і QLC+.

## Команди

```bash
cargo test --workspace                                   # усі тести
cargo clippy --workspace --all-targets -- -D warnings    # як у CI
cargo fmt --all
cargo check --workspace --target x86_64-pc-windows-gnu   # код під Windows (потрібен mingw-w64)
cargo run -p nitor -- --config /tmp/cfg.json             # програма з окремими налаштуваннями
```

Перевірка вигляду без Windows: `NITOR_HOME=<тека> NITOR_SCREENSHOT=знімок.png [NITOR_SCREENSHOT_ACTS=selectAll,settings]
xvfb-run -a ./target/debug/nitor` — програма зберігає знімок вікна й закривається (дії: `settings`, `patch`, `keys`, `midi`,
`add`, `library`, `props`, `close`, `monitor`, `menu`, `flash`). Вікно запуску — `NITOR_SCREENSHOT_SPLASH=1`; фальшивий реліз —
`NITOR_UPDATE_API=http://127.0.0.1:…/latest.json NITOR_UPDATE_ALLOW=1`. Інсталятор знімається так само (`NITOR_SCREENSHOT`).

## Правила

- **Тексти інтерфейсу** — українською, з великої літери, без емодзі. Кольори, шрифти, відступи й радіуси —
  лише з токенів `nitor-ui/src/theme.rs` (`BG`, `SURFACE`, `ACCENT`…). Один основний (синій) колір дії на екрані;
  червоний — лише небезпечні дії. Контраст тексту ≥ 4.5:1.
- **Поведінка рушія**: зміни в `render`, ефектах чи кросфейді мають проходити `tests/engine.rs`; навмисну зміну
  поведінки покривайте тестом.
- **Лише Rust**: увесь код програми й інсталятора — Rust; інші мови тільки там, де без них не обійтися
  (і з поясненням чому). Нові залежності — переважно чисті Rust-крейти без C-бібліотек.
- **Стиль роботи** — навичка `/ponytail` (`.claude/skills/ponytail`, MIT, з github.com/dietrichgebert/ponytail):
  найменша повна зміна, без зайвих абстракцій; обхідні рішення позначаються `// shortcut: <межа>, <коли переробити>`.
  Огляд змін — `/ponytail-review`, борг — `/ponytail-debt`.
- **Без Windows-специфіки в `nitor-core`**; Windows API — лише в `winshell.rs`, `nitor-setup/src/sys.rs` під `cfg(windows)`
  із заглушками для інших систем (workspace має збиратися й під Linux).
- **Одна копія програми**: м'ютекс `Local\Nitor.App`. Інсталятор закриває програму подією `Local\Nitor.Quit`.
- **Оновлення**: програма завантажує `Nitor-Setup.exe` з релізу й запускає `/update --wait-pid`; світло під час
  оновлення не гасне (`core.shutdown(false)`).

## Релізи

- Реліз створює `.github/workflows/release.yml`, щойно в `main` потрапляє нова `version` з `[workspace.package]`
  у `Cargo.toml` (текст релізу — розділ `## X.Y.Z` з `CHANGELOG.md`). До релізу додається `Nitor-Setup.exe`.
  Програми користувачів оновлюються лише на **новішу** версію.
- **Ніколи не пушити в `main` без явного «так» користувача**: кожен push у `main` з новою версією — реліз для всіх.
  Роботу — у гілці; для випуску є навичка `/release`.
- `.github/workflows/windows-check.yml` на кожен push у гілку (крім `main`) збирає програму й інсталятор, ставить
  її на справжньому Windows і перевіряє ярлики, ідентифікатор програми, одну копію, вікно запуску,
  повторне встановлення поверх відкритої програми й вибіркове видалення. Знімки — в журналі рядками `SHOT|назва|base64`.

## Підпис коду

Інсталятор не підписаний (SmartScreen показує попередження).
