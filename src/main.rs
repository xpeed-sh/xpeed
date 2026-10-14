#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod about;
mod app;
mod assistant;
mod connections;
mod credentials;
mod directory;
mod extensions;
mod file_explorer;
mod history;
mod input;
mod palette;
mod productivity;
mod server_monitor;
mod settings;
mod sftp;
mod svg_icons;
mod terminal;
mod terminal_agent;
mod terminal_view;
mod updates;
mod window_size;
mod workspace;
fn main() -> eframe::Result {
    if std::env::args().any(|arg| arg == "--monitor-smoke-test") {
        let args: Vec<_> = std::env::args().collect();
        let result = (|| -> Result<(), String> {
            let config = serde_json::from_slice(
                &std::fs::read(args.get(2).ok_or("Config missing")?).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let hosts = std::path::PathBuf::from(args.get(3).ok_or("Hosts missing")?);
            let password = credentials::Secret::new(
                std::env::var("XPEED_TEST_PASSWORD").map_err(|e| e.to_string())?,
            );
            server_monitor::smoke_test(config, password, hosts)
        })();
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--install-extension") {
        let args: Vec<_> = std::env::args().collect();
        let result = args
            .windows(2)
            .find(|pair| pair[0] == "--install-extension")
            .ok_or_else(|| xpeed_i18n::t("Falta la carpeta del paquete").to_string())
            .and_then(|pair| {
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                extensions::Extensions::new(&cwd, false).install(std::path::Path::new(&pair[1]))
            });
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if std::env::args().any(|a| a == "--extension-smoke-test") {
        let args: Vec<String> = std::env::args().collect();
        let result = args
            .get(2)
            .ok_or_else(|| xpeed_i18n::t("Falta la carpeta de la extensión").to_string())
            .and_then(|path| extensions::smoke_test(std::path::Path::new(path)));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(code) = credentials::helper() {
        std::process::exit(code);
    }
    if std::env::args().any(|a| a == "--ssh-smoke-test") {
        let args: Vec<String> = std::env::args().collect();
        let result = (|| -> Result<(), String> {
            let config: connections::SshConfig = serde_json::from_slice(
                &std::fs::read(args.get(2).ok_or("Config missing")?).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let hosts = std::path::Path::new(args.get(3).ok_or("Known hosts missing")?);
            let password = credentials::Secret::new(
                std::env::var("XPEED_TEST_PASSWORD").map_err(|e| e.to_string())?,
            );
            terminal::ssh_smoke_test(&config, password.clone(), hosts)?;
            sftp::smoke_test(&config, password, hosts)
        })();
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if std::env::args().any(|a| a == "--smoke-test") {
        if let Err(error) = terminal::smoke_test() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    xpeed_i18n::set_language(settings::Settings::load().language);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(xpeed_i18n::t("Xpeed — Conexiones y terminales"))
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([720.0, 440.0])
            .with_icon(icon()),
        renderer: eframe::Renderer::Glow,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "Xpeed",
        options,
        Box::new(|cc| Ok(Box::new(app::Xpeed::new(cc)))),
    )
}
fn icon() -> eframe::egui::IconData {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!(
        "../assets/img/app-icon.png"
    )));
    let mut reader = decoder.read_info().expect("Embedded application icon PNG");
    let mut rgba = vec![0; reader.output_buffer_size().expect("Application icon size")];
    let info = reader
        .next_frame(&mut rgba)
        .expect("Application icon pixels");
    assert_eq!(info.color_type, png::ColorType::Rgba);
    rgba.truncate(info.buffer_size());
    eframe::egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    }
}
