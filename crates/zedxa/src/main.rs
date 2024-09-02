// Allow binary to be called Zed for a nice application menu when running executable directly
#![allow(non_snake_case)]
// Disable command line from opening on release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod reliability;
mod zed;

use anyhow::{anyhow, Context as _, Result};
use chrono::Offset;
use clap::{command, Parser};
use cli::FORCE_CLI_MODE_ENV_VAR_NAME;
use client::{Client, UserStore};
use db::kvp::KEY_VALUE_STORE;
use editor::Editor;
use env_logger::Builder;
use fs::{Fs, RealFs};
use git::GitHostingProviderRegistry;
use gpui::{
    Action, App, AppContext, AsyncAppContext, Context, DismissEvent, Global, Task, VisualContext,
};
use language::LanguageRegistry;
use log::LevelFilter;
use uuid::Uuid;

use assets::Assets;
use node_runtime::RealNodeRuntime;
use parking_lot::Mutex;
use release_channel::{AppCommitSha, AppVersion};
use session::{AppSession, Session};
use settings::{handle_settings_file_changes, watch_config_file, Settings};
use simplelog::ConfigBuilder;
use std::{
    env,
    fs::OpenOptions,
    io::{IsTerminal, Write},
    process,
    sync::Arc,
};
use theme::{ActiveTheme, SystemAppearance};
use time::UtcOffset;
use util::ResultExt;
use welcome::{show_welcome_view, FIRST_OPEN};
use workspace::{
    notifications::{simple_message_notification::MessageNotification, NotificationId},
    AppState, WorkspaceSettings, WorkspaceStore,
};
use zed::{app_menus, build_window_options, handle_keymap_file_changes};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn fail_to_launch(e: anyhow::Error) {
    eprintln!("Zed failed to launch: {e:?}");
    App::new().run(move |cx| {
        if let Ok(window) = cx.open_window(gpui::WindowOptions::default(), |cx| cx.new_view(|_| gpui::Empty)) {
            window.update(cx, |_, cx| {
                let response = cx.prompt(gpui::PromptLevel::Critical, "Zed failed to launch", Some(&format!("{e}\n\nFor help resolving this, please open an issue on https://github.com/zed-industries/zed")), &["Exit"]);

                cx.spawn(|_, mut cx| async move {
                    response.await?;
                    cx.update(|cx| {
                        cx.quit()
                    })
                }).detach_and_log_err(cx);
            }).log_err();
        } else {
            fail_to_open_window(e, cx)
        }
    })
}

fn fail_to_open_window_async(e: anyhow::Error, cx: &mut AsyncAppContext) {
    cx.update(|cx| fail_to_open_window(e, cx)).log_err();
}

fn fail_to_open_window(e: anyhow::Error, _cx: &mut AppContext) {
    eprintln!(
        "Zed failed to open a window: {e:?}. See https://zed.dev/docs/linux for troubleshooting steps."
    );
    #[cfg(not(target_os = "linux"))]
    {
        process::exit(1);
    }
}

enum AppMode {
    Ui,
}
impl Global for AppMode {}

// init_common is called for both headless and normal mode.
fn init_common(_app_state: Arc<AppState>, cx: &mut AppContext) {
    SystemAppearance::init(cx);
    theme::init(theme::LoadThemes::All(Box::new(Assets)), cx);
    command_palette::init(cx);
    /*
    language_model::init(
        app_state.user_store.clone(),
        app_state.client.clone(),
        app_state.fs.clone(),
        cx,
    );
    snippet_provider::init(cx);
    repl::init(
        app_state.fs.clone(),
        app_state.client.telemetry().clone(),
        cx,
    );
    */
}

fn init_ui(app_state: Arc<AppState>, cx: &mut AppContext) -> Result<()> {
    match cx.try_global::<AppMode>() {
        Some(AppMode::Ui) => return Ok(()),
        None => {
            cx.set_global(AppMode::Ui);
        }
    };

    load_embedded_fonts(cx);

    app_state.languages.set_theme(cx.theme().clone());
    editor::init(cx);
    //diagnostics::init(cx);

    workspace::init(app_state.clone(), cx);

    /*
    recent_projects::init(cx);
    go_to_line::init(cx);
    file_finder::init(cx);
    */
    tab_switcher::init(cx);
    /*
    outline::init(cx);
    project_symbols::init(cx);
    project_panel::init(Assets, cx);
    outline_panel::init(Assets, cx);
    tasks_ui::init(cx);
    channel::init(&app_state.client.clone(), app_state.user_store.clone(), cx);
    search::init(cx);
    vim::init(cx);
    terminal_view::init(cx);
    journal::init(app_state.clone(), cx);
    language_selector::init(cx);
    */
    theme_selector::init(cx);
    /*
    language_tools::init(cx);
    call::init(app_state.client.clone(), app_state.user_store.clone(), cx);
    notifications::init(app_state.client.clone(), app_state.user_store.clone(), cx);
    feedback::init(cx);
    markdown_preview::init(cx);
    */
    welcome::init(cx);
    //settings_ui::init(cx);
    //performance::init(cx);

    /*
    cx.observe_global::<SettingsStore>({
        let languages = app_state.languages.clone();
        let http = app_state.client.http_client();
        let client = app_state.client.clone();

        move |cx| {
            for &mut window in cx.windows().iter_mut() {
                let background_appearance = cx.theme().window_background_appearance();
                window
                    .update(cx, |_, cx| {
                        cx.set_background_appearance(background_appearance)
                    })
                    .ok();
            }
            languages.set_theme(cx.theme().clone());
            let new_host = &client::ClientSettings::get_global(cx).server_url;
            if &http.base_url() != new_host {
                http.set_base_url(new_host);
                if client.status().borrow().is_connected() {
                    client.reconnect(&cx.to_async());
                }
            }
        }
    })
    .detach();
    let fs = app_state.fs.clone();
    load_user_themes_in_background(fs.clone(), cx);
    watch_themes(fs.clone(), cx);
    watch_languages(fs.clone(), app_state.languages.clone(), cx);
    watch_file_types(fs.clone(), cx);
    */
    cx.set_menus(app_menus());
    //initialize_workspace(app_state.clone(), cx);

    cx.activate(true);

    Ok(())
}

fn main() {
    let start_time = std::time::Instant::now();
    menu::init();
    zed_actions::init();

    if let Err(e) = init_paths() {
        fail_to_launch(e);
        return;
    }

    init_logger();

    log::info!("========== starting zedxa ==========");
    let app = App::new()
        .with_assets(Assets)
        .measure_time_to_first_window_draw(start_time);

    let (installation_id, _) = app
        .background_executor()
        .block(installation_id())
        .ok()
        .unzip();

    let session = app.background_executor().block(Session::new());

    let app_version = AppVersion::init(env!("CARGO_PKG_VERSION"));

    reliability::init_panic_hook(
        installation_id.clone(),
        app_version,
        session.id().to_owned(),
    );

    let git_hosting_provider_registry = Arc::new(GitHostingProviderRegistry::new());
    let git_binary_path =
        if cfg!(target_os = "macos") && option_env!("ZED_BUNDLE").as_deref() == Some("true") {
            app.path_for_auxiliary_executable("git")
                .context("could not find git binary path")
                .log_err()
        } else {
            None
        };
    log::info!("Using git binary path: {:?}", git_binary_path);

    let fs = Arc::new(RealFs::new(
        git_hosting_provider_registry.clone(),
        git_binary_path,
    ));
    let user_settings_file_rx = watch_config_file(
        &app.background_executor(),
        fs.clone(),
        paths::settings_file().clone(),
    );
    let user_keymap_file_rx = watch_config_file(
        &app.background_executor(),
        fs.clone(),
        paths::keymap_file().clone(),
    );

    let login_shell_env_loaded = Task::ready(());

    app.on_reopen(move |cx| {
        if let Some(app_state) = AppState::try_global(cx).and_then(|app_state| app_state.upgrade())
        {
            cx.spawn({
                let app_state = app_state.clone();
                |mut cx| async move {
                    if let Err(e) = restore_or_create_workspace(app_state, &mut cx).await {
                        fail_to_open_window_async(e, &mut cx)
                    }
                }
            })
            .detach();
        }
    });

    app.run(move |cx| {
        release_channel::init(app_version, cx);
        if let Some(build_sha) = option_env!("ZED_COMMIT_SHA") {
            AppCommitSha::set_global(AppCommitSha(build_sha.into()), cx);
        }

        <dyn Fs>::set_global(fs.clone(), cx);

        GitHostingProviderRegistry::set_global(git_hosting_provider_registry, cx);
        git_hosting_providers::init(cx);

        settings::init(cx);
        handle_settings_file_changes(user_settings_file_rx, cx, handle_settings_changed);
        handle_keymap_file_changes(user_keymap_file_rx, cx, handle_keymap_changed);

        client::init_settings(cx);
        let client = Client::production(cx);
        cx.set_http_client(client.http_client().clone());
        let mut languages =
            LanguageRegistry::new(login_shell_env_loaded, cx.background_executor().clone());
        languages.set_language_server_download_dir(paths::languages_dir().clone());
        let languages = Arc::new(languages);
        let node_runtime = RealNodeRuntime::new(client.http_client());

        language::init(cx);
        languages::init(languages.clone(), node_runtime.clone(), cx);
        let user_store = cx.new_model(|cx| UserStore::new(client.clone(), cx));
        let workspace_store = cx.new_model(|cx| WorkspaceStore::new(client.clone(), cx));

        Client::set_global(client.clone(), cx);

        zed::init(cx);
        project::Project::init(&client, cx);
        client::init(&client, cx);
        language::init(cx);
        let app_session = cx.new_model(|cx| AppSession::new(session, cx));

        let app_state = Arc::new(AppState {
            languages: languages.clone(),
            client: client.clone(),
            user_store: user_store.clone(),
            fs: fs.clone(),
            build_window_options,
            workspace_store,
            node_runtime: node_runtime.clone(),
            session: app_session,
        });
        AppState::set_global(Arc::downgrade(&app_state), cx);

        reliability::init(client.http_client(), installation_id, cx);

        init_common(app_state.clone(), cx);

        init_ui(app_state.clone(), cx).unwrap();
        cx.spawn({
            let app_state = app_state.clone();
            |mut cx| async move {
                if let Err(e) = restore_or_create_workspace(app_state, &mut cx).await {
                    fail_to_open_window_async(e, &mut cx)
                }
            }
        })
        .detach();
    });
}

fn handle_keymap_changed(error: Option<anyhow::Error>, cx: &mut AppContext) {
    struct KeymapParseErrorNotification;
    let id = NotificationId::unique::<KeymapParseErrorNotification>();

    for workspace in workspace::local_workspace_windows(cx) {
        workspace
            .update(cx, |workspace, cx| match &error {
                Some(error) => {
                    workspace.show_notification(id.clone(), cx, |cx| {
                        cx.new_view(|_| {
                            MessageNotification::new(format!("Invalid keymap file\n{error}"))
                                .with_click_message("Open keymap file")
                                .on_click(|cx| {
                                    cx.dispatch_action(zed_actions::OpenKeymap.boxed_clone());
                                    cx.emit(DismissEvent);
                                })
                        })
                    });
                }
                None => workspace.dismiss_notification(&id, cx),
            })
            .log_err();
    }
}

fn handle_settings_changed(error: Option<anyhow::Error>, cx: &mut AppContext) {
    struct SettingsParseErrorNotification;
    let id = NotificationId::unique::<SettingsParseErrorNotification>();

    for workspace in workspace::local_workspace_windows(cx) {
        workspace
            .update(cx, |workspace, cx| match &error {
                Some(error) => {
                    workspace.show_notification(id.clone(), cx, |cx| {
                        cx.new_view(|_| {
                            MessageNotification::new(format!("Invalid settings file\n{error}"))
                                .with_click_message("Open settings file")
                                .on_click(|cx| {
                                    cx.dispatch_action(zed_actions::OpenSettings.boxed_clone());
                                    cx.emit(DismissEvent);
                                })
                        })
                    });
                }
                None => workspace.dismiss_notification(&id, cx),
            })
            .log_err();
    }
}

async fn installation_id() -> Result<(String, bool)> {
    let legacy_key_name = "device_id".to_string();
    let key_name = "installation_id".to_string();

    // Migrate legacy key to new key
    if let Ok(Some(installation_id)) = KEY_VALUE_STORE.read_kvp(&legacy_key_name) {
        KEY_VALUE_STORE
            .write_kvp(key_name, installation_id.clone())
            .await?;
        KEY_VALUE_STORE.delete_kvp(legacy_key_name).await?;
        return Ok((installation_id, true));
    }

    if let Ok(Some(installation_id)) = KEY_VALUE_STORE.read_kvp(&key_name) {
        return Ok((installation_id, true));
    }

    let installation_id = Uuid::new_v4().to_string();

    KEY_VALUE_STORE
        .write_kvp(key_name, installation_id.clone())
        .await?;

    Ok((installation_id, false))
}

async fn restore_or_create_workspace(
    app_state: Arc<AppState>,
    cx: &mut AsyncAppContext,
) -> Result<()> {
    if let Some(locations) = restorable_workspace_locations(cx, &app_state).await {
        for location in locations {
            cx.update(|cx| {
                workspace::open_paths(
                    location.paths().as_ref(),
                    app_state.clone(),
                    workspace::OpenOptions::default(),
                    cx,
                )
            })?
            .await?;
        }
    } else if matches!(KEY_VALUE_STORE.read_kvp(FIRST_OPEN), Ok(None)) {
        cx.update(|cx| show_welcome_view(app_state, cx))?.await?;
    } else {
        cx.update(|cx| {
            workspace::open_new(Default::default(), app_state, cx, |workspace, cx| {
                Editor::new_file(workspace, &Default::default(), cx)
            })
        })?
        .await?;
    }

    Ok(())
}

pub(crate) async fn restorable_workspace_locations(
    cx: &mut AsyncAppContext,
    app_state: &Arc<AppState>,
) -> Option<Vec<workspace::LocalPaths>> {
    let mut restore_behavior = cx
        .update(|cx| WorkspaceSettings::get(None, cx).restore_on_startup)
        .ok()?;

    let session_handle = app_state.session.clone();
    let (last_session_id, last_session_window_stack) = cx
        .update(|cx| {
            let session = session_handle.read(cx);

            (
                session.last_session_id().map(|id| id.to_string()),
                session.last_session_window_stack(),
            )
        })
        .ok()?;

    if last_session_id.is_none()
        && matches!(
            restore_behavior,
            workspace::RestoreOnStartupBehavior::LastSession
        )
    {
        restore_behavior = workspace::RestoreOnStartupBehavior::LastWorkspace;
    }

    match restore_behavior {
        workspace::RestoreOnStartupBehavior::LastWorkspace => {
            workspace::last_opened_workspace_paths()
                .await
                .map(|location| vec![location])
        }
        workspace::RestoreOnStartupBehavior::LastSession => {
            if let Some(last_session_id) = last_session_id {
                let ordered = last_session_window_stack.is_some();

                let mut locations = workspace::last_session_workspace_locations(
                    &last_session_id,
                    last_session_window_stack,
                )
                .filter(|locations| !locations.is_empty());

                // Since last_session_window_order returns the windows ordered front-to-back
                // we need to open the window that was frontmost last.
                if ordered {
                    if let Some(locations) = locations.as_mut() {
                        locations.reverse();
                    }
                }

                locations
            } else {
                None
            }
        }
        _ => None,
    }
}

fn init_paths() -> anyhow::Result<()> {
    for path in [
        paths::config_dir(),
        //paths::extensions_dir(),
        paths::languages_dir(),
        paths::database_dir(),
        paths::logs_dir(),
        paths::temp_dir(),
    ]
    .iter()
    {
        std::fs::create_dir_all(path)
            .map_err(|e| anyhow!("Could not create directory {:?}: {}", path, e))?;
    }
    Ok(())
}

fn init_logger() {
    if stdout_is_a_pty() {
        init_stdout_logger();
    } else {
        let level = LevelFilter::Info;

        // Prevent log file from becoming too large.
        const KIB: u64 = 1024;
        const MIB: u64 = 1024 * KIB;
        const MAX_LOG_BYTES: u64 = MIB;
        if std::fs::metadata(paths::log_file())
            .map_or(false, |metadata| metadata.len() > MAX_LOG_BYTES)
        {
            let _ = std::fs::rename(paths::log_file(), paths::old_log_file());
        }

        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths::log_file())
        {
            Ok(log_file) => {
                let mut config_builder = ConfigBuilder::new();

                config_builder.set_time_format_rfc3339();
                let local_offset = chrono::Local::now().offset().fix().local_minus_utc();
                if let Ok(offset) = UtcOffset::from_whole_seconds(local_offset) {
                    config_builder.set_time_offset(offset);
                }

                #[cfg(target_os = "linux")]
                {
                    config_builder.add_filter_ignore_str("zbus");
                    config_builder.add_filter_ignore_str("blade_graphics::hal::resource");
                    config_builder.add_filter_ignore_str("naga::back::spv::writer");
                }

                let config = config_builder.build();
                simplelog::WriteLogger::init(level, config, log_file)
                    .expect("could not initialize logger");
            }
            Err(err) => {
                init_stdout_logger();
                log::error!(
                    "could not open log file, defaulting to stdout logging: {}",
                    err
                );
            }
        }
    }
}

fn init_stdout_logger() {
    Builder::new()
        .parse_default_env()
        .format(|buf, record| {
            use env_logger::fmt::style::{AnsiColor, Style};

            let subtle = Style::new().fg_color(Some(AnsiColor::BrightBlack.into()));
            write!(buf, "{subtle}[{subtle:#}")?;
            write!(
                buf,
                "{} ",
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%:z")
            )?;
            let level_style = buf.default_level_style(record.level());
            write!(buf, "{level_style}{:<5}{level_style:#}", record.level())?;
            if let Some(path) = record.module_path() {
                write!(buf, " {path}")?;
            }
            write!(buf, "{subtle}]{subtle:#}")?;
            writeln!(buf, " {}", record.args())
        })
        .init();
}

fn stdout_is_a_pty() -> bool {
    std::env::var(FORCE_CLI_MODE_ENV_VAR_NAME).ok().is_none() && std::io::stdout().is_terminal()
}

#[derive(Parser, Debug)]
#[command(name = "zed", disable_version_flag = true)]
struct Args {
    /// A sequence of space-separated paths or urls that you want to open.
    ///
    /// Use `path:line:row` syntax to open a file at a specific location.
    /// Non-existing paths and directories will ignore `:line:row` suffix.
    ///
    /// URLs can either be `file://` or `zed://` scheme, or relative to <https://zed.dev>.
    paths_or_urls: Vec<String>,

    /// Instructs zed to run as a dev server on this machine. (not implemented)
    #[arg(long)]
    dev_server_token: Option<String>,
}

fn load_embedded_fonts(cx: &AppContext) {
    let asset_source = cx.asset_source();
    let font_paths = asset_source.list("fonts").unwrap();
    let embedded_fonts = Mutex::new(Vec::new());
    let executor = cx.background_executor();

    executor.block(executor.scoped(|scope| {
        for font_path in &font_paths {
            if !font_path.ends_with(".ttf") {
                continue;
            }

            scope.spawn(async {
                let font_bytes = asset_source.load(font_path).unwrap().unwrap();
                embedded_fonts.lock().push(font_bytes);
            });
        }
    }));

    cx.text_system()
        .add_fonts(embedded_fonts.into_inner())
        .unwrap();
}

/*
/// Spawns a background task to load the user themes from the themes directory.
fn load_user_themes_in_background(fs: Arc<dyn fs::Fs>, cx: &mut AppContext) {
    cx.spawn({
        let fs = fs.clone();
        |cx| async move {
            if let Some(theme_registry) =
                cx.update(|cx| ThemeRegistry::global(cx).clone()).log_err()
            {
                let themes_dir = paths::themes_dir().as_ref();
                match fs
                    .metadata(themes_dir)
                    .await
                    .ok()
                    .flatten()
                    .map(|m| m.is_dir)
                {
                    Some(is_dir) => {
                        anyhow::ensure!(is_dir, "Themes dir path {themes_dir:?} is not a directory")
                    }
                    None => {
                        fs.create_dir(themes_dir).await.with_context(|| {
                            format!("Failed to create themes dir at path {themes_dir:?}")
                        })?;
                    }
                }
                theme_registry.load_user_themes(themes_dir, fs).await?;
                cx.update(|cx| ThemeSettings::reload_current_theme(cx))?;
            }
            anyhow::Ok(())
        }
    })
    .detach_and_log_err(cx);
}

/// Spawns a background task to watch the themes directory for changes.
fn watch_themes(fs: Arc<dyn fs::Fs>, cx: &mut AppContext) {
    use std::time::Duration;
    cx.spawn(|cx| async move {
        let (mut events, _) = fs
            .watch(paths::themes_dir(), Duration::from_millis(100))
            .await;

        while let Some(paths) = events.next().await {
            for path in paths {
                if fs.metadata(&path).await.ok().flatten().is_some() {
                    if let Some(theme_registry) =
                        cx.update(|cx| ThemeRegistry::global(cx).clone()).log_err()
                    {
                        if let Some(()) = theme_registry
                            .load_user_theme(&path, fs.clone())
                            .await
                            .log_err()
                        {
                            cx.update(|cx| ThemeSettings::reload_current_theme(cx))
                                .log_err();
                        }
                    }
                }
            }
        }
    })
    .detach()
}





#[cfg(debug_assertions)]
fn watch_languages(fs: Arc<dyn fs::Fs>, languages: Arc<LanguageRegistry>, cx: &mut AppContext) {
    use std::time::Duration;

    let path = {
        let p = Path::new("crates/languages/src");
        let Ok(full_path) = p.canonicalize() else {
            return;
        };
        full_path
    };

    cx.spawn(|_| async move {
        let (mut events, _) = fs.watch(path.as_path(), Duration::from_millis(100)).await;
        while let Some(event) = events.next().await {
            let has_language_file = event.iter().any(|path| {
                path.extension()
                    .map(|ext| ext.to_string_lossy().as_ref() == "scm")
                    .unwrap_or(false)
            });
            if has_language_file {
                languages.reload();
            }
        }
    })
    .detach()
}

#[cfg(not(debug_assertions))]
fn watch_languages(_fs: Arc<dyn fs::Fs>, _languages: Arc<LanguageRegistry>, _cx: &mut AppContext) {}

#[cfg(debug_assertions)]
fn watch_file_types(fs: Arc<dyn fs::Fs>, cx: &mut AppContext) {
    use std::time::Duration;

    use file_icons::FileIcons;
    use gpui::UpdateGlobal;

    let path = {
        let p = Path::new("assets/icons/file_icons/file_types.json");
        let Ok(full_path) = p.canonicalize() else {
            return;
        };
        full_path
    };

    cx.spawn(|cx| async move {
        let (mut events, _) = fs.watch(path.as_path(), Duration::from_millis(100)).await;
        while (events.next().await).is_some() {
            cx.update(|cx| {
                FileIcons::update_global(cx, |file_types, _cx| {
                    *file_types = file_icons::FileIcons::new(Assets);
                });
            })
            .ok();
        }
    })
    .detach()
}

#[cfg(not(debug_assertions))]
fn watch_file_types(_fs: Arc<dyn fs::Fs>, _cx: &mut AppContext) {}
*/
