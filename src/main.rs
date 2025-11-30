//! Git Worktree Agent - A TUI for managing git worktrees from remote branches
//!
//! This application watches a remote repository for new branches and automatically
//! creates local worktrees for them, running configured commands (like `npm install`)
//! after each worktree is created.

mod app;
mod config;
mod executor;
mod git;
mod ui;
mod watcher;

use clap::Parser;
use color_eyre::eyre::Result;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Git Worktree Agent - Manage git worktrees from remote branches
#[derive(Parser, Debug)]
#[command(name = "gwa")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the git repository (defaults to current directory)
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Enable debug logging (writes to gwa-debug.log)
    #[arg(short, long)]
    debug: bool,

    /// Initialize configuration interactively
    #[arg(long)]
    init: bool,

    /// Set the post-create command (e.g., "npm install")
    #[arg(long)]
    set_command: Option<String>,

    /// Set the poll interval in seconds
    #[arg(long)]
    set_poll_interval: Option<u64>,

    /// Enable auto-create mode
    #[arg(long)]
    auto_create: bool,

    /// Print the current configuration
    #[arg(long)]
    show_config: bool,
}

fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize color_eyre for better error reporting
    color_eyre::install()?;

    // Determine repository path
    let repo_path = args.path.clone().unwrap_or_else(|| {
        std::env::current_dir().expect("Failed to get current directory")
    });

    // Check if we're running in TUI mode
    let is_tui_mode = !args.show_config
        && !args.init
        && args.set_command.is_none()
        && args.set_poll_interval.is_none()
        && !args.auto_create;

    // Initialize tracing/logging
    // In TUI mode, only log to file if debug is enabled
    // In non-TUI mode, log to console
    if is_tui_mode {
        if args.debug {
            // Write debug logs to a file when running TUI
            let log_file = std::fs::File::create(repo_path.join("gwa-debug.log"))
                .expect("Failed to create log file");
            let filter = EnvFilter::new("debug");
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_target(false)
                        .with_ansi(false)
                        .with_writer(log_file),
                )
                .init();
        }
        // If not debug, don't initialize any logging - TUI handles status display
    } else {
        // For CLI commands, log to console
        let filter = if args.debug {
            EnvFilter::new("debug")
        } else {
            EnvFilter::new("warn")
        };
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().with_target(false))
            .init();
    }

    // Handle non-TUI commands
    if args.show_config {
        return show_config(&repo_path);
    }

    if args.set_command.is_some() || args.set_poll_interval.is_some() || args.auto_create {
        return update_config(&repo_path, &args);
    }

    if args.init {
        return init_config(&repo_path);
    }

    // Run the TUI application
    let terminal = ratatui::init();
    
    // Enable mouse capture for scroll wheel (use Shift+click for text selection)
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture).ok();
    
    let result = match app::App::new(&repo_path) {
        Ok(app) => app.run(terminal),
        Err(e) => {
            // Show error in TUI before exiting
            show_startup_error(terminal, &e.to_string());
            Ok(())
        }
    };
    
    // Ensure mouse capture is disabled on exit
    crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture).ok();
    ratatui::restore();
    result
}

/// Show a startup error in a TUI dialog
fn show_startup_error(mut terminal: ratatui::DefaultTerminal, error_msg: &str) {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use ratatui::{
        layout::Rect,
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Clear, Paragraph, Wrap},
    };

    let theme_error = Color::Rgb(237, 135, 150);
    let theme_primary = Color::Rgb(138, 180, 248);
    let theme_fg = Color::Rgb(205, 214, 244);
    let theme_muted = Color::Rgb(108, 112, 134);
    let theme_bg = Color::Rgb(30, 30, 46);

    loop {
        terminal
            .draw(|frame| {
                let area = frame.area();

                // Fill background
                frame.render_widget(
                    Block::default().style(Style::default().bg(theme_bg)),
                    area,
                );

                // Calculate popup size
                let popup_width = 70.min(area.width.saturating_sub(4));
                let popup_height = 18.min(area.height.saturating_sub(4));
                let popup_x = (area.width.saturating_sub(popup_width)) / 2;
                let popup_y = (area.height.saturating_sub(popup_height)) / 2;

                let popup_area = Rect {
                    x: area.x + popup_x,
                    y: area.y + popup_y,
                    width: popup_width,
                    height: popup_height,
                };

                frame.render_widget(Clear, popup_area);

                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme_error))
                    .style(Style::default().bg(theme_bg))
                    .title(Span::styled(
                        " ⚠ Startup Error ",
                        Style::default()
                            .fg(theme_error)
                            .add_modifier(Modifier::BOLD),
                    ));

                let inner = block.inner(popup_area);

                // Format error message
                let mut lines: Vec<Line> = vec![Line::raw("")];

                // Clean up the error message (remove color_eyre formatting)
                let clean_msg = error_msg
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .filter(|l| !l.contains("Location:"))
                    .filter(|l| !l.contains("Backtrace"))
                    .filter(|l| !l.contains("RUST_BACKTRACE"))
                    .filter(|l| !l.contains("src/"))
                    .map(|l| l.trim())
                    .collect::<Vec<_>>()
                    .join("\n");

                for line in clean_msg.lines().take(10) {
                    lines.push(Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(theme_fg),
                    )));
                }

                lines.push(Line::raw(""));
                lines.push(Line::from(Span::styled(
                    "Make sure you run gwa from within a git repository,",
                    Style::default().fg(theme_muted),
                )));
                lines.push(Line::from(Span::styled(
                    "or specify the path with: gwa --path /path/to/repo",
                    Style::default().fg(theme_muted),
                )));
                lines.push(Line::raw(""));
                lines.push(Line::raw(""));
                lines.push(Line::from(vec![
                    Span::styled("Press ", Style::default().fg(theme_muted)),
                    Span::styled(
                        "q",
                        Style::default()
                            .fg(theme_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" or ", Style::default().fg(theme_muted)),
                    Span::styled(
                        "Esc",
                        Style::default()
                            .fg(theme_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" to exit", Style::default().fg(theme_muted)),
                ]));

                frame.render_widget(block, popup_area);
                frame.render_widget(
                    Paragraph::new(lines).wrap(Wrap { trim: false }),
                    inner,
                );
            })
            .ok();

        // Wait for quit key
        if let Ok(true) = event::poll(std::time::Duration::from_millis(100)) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Show the current configuration
fn show_config(repo_path: &PathBuf) -> Result<()> {
    let repo = git::Repository::discover(repo_path)?;
    let config = config::Config::load(repo.root())?;

    println!("Git Worktree Agent Configuration");
    println!("================================");
    println!();
    println!("Remote: {}", config.remote_name);
    println!("Poll interval: {}s", config.poll_interval_secs);
    println!("Auto-create: {}", config.auto_create_worktrees);
    println!("Worktree base: {}", config.worktree_base_dir);
    println!(
        "Post-create command: {}",
        config.post_create_command.as_deref().unwrap_or("(none)")
    );
    println!();
    println!("Tracked branches ({}):", config.tracked_branches.len());
    for branch in &config.tracked_branches {
        println!("  + {}", branch);
    }
    println!();
    println!("Untracked branches ({}):", config.untracked_branches.len());
    for branch in &config.untracked_branches {
        println!("  - {}", branch);
    }
    println!();
    println!("Ignore patterns ({}):", config.ignore_patterns.len());
    for pattern in &config.ignore_patterns {
        println!("  * {}", pattern);
    }
    println!();
    println!("Active worktrees ({}):", config.worktrees.len());
    for wt in &config.worktrees {
        println!("  {} -> {}", wt.branch, wt.path.display());
    }

    Ok(())
}

/// Update configuration from command line
fn update_config(repo_path: &PathBuf, args: &Args) -> Result<()> {
    let repo = git::Repository::discover(repo_path)?;
    let mut config = config::Config::load(repo.root())?;

    if let Some(ref cmd) = args.set_command {
        config.post_create_command = Some(cmd.clone());
        println!("Set post-create command: {}", cmd);
    }

    if let Some(interval) = args.set_poll_interval {
        config.poll_interval_secs = interval;
        println!("Set poll interval: {}s", interval);
    }

    if args.auto_create {
        config.auto_create_worktrees = true;
        println!("Enabled auto-create mode");
    }

    config.save(repo.root())?;
    println!("Configuration saved to {}", config::CONFIG_FILE_NAME);

    Ok(())
}

/// Initialize configuration interactively
fn init_config(repo_path: &PathBuf) -> Result<()> {
    use std::io::{self, Write};

    let repo = git::Repository::discover(repo_path)?;
    let mut config = config::Config::load(repo.root())?;

    println!("Git Worktree Agent - Configuration");
    println!("===================================");
    println!();

    // Get remote name
    print!("Remote name [{}]: ", config.remote_name);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if !input.is_empty() {
        config.remote_name = input.to_string();
    }

    // Get poll interval
    print!("Poll interval in seconds [{}]: ", config.poll_interval_secs);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if !input.is_empty() {
        if let Ok(interval) = input.parse() {
            config.poll_interval_secs = interval;
        }
    }

    // Get post-create command
    print!(
        "Post-create command (e.g., 'npm install') [{}]: ",
        config.post_create_command.as_deref().unwrap_or("")
    );
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if !input.is_empty() {
        config.post_create_command = Some(input.to_string());
    }

    // Get auto-create setting
    print!(
        "Auto-create worktrees for new branches? [{}]: ",
        if config.auto_create_worktrees { "y" } else { "n" }
    );
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    if input == "y" || input == "yes" {
        config.auto_create_worktrees = true;
    } else if input == "n" || input == "no" {
        config.auto_create_worktrees = false;
    }

    // Get worktree base directory
    print!("Worktree base directory [{}]: ", config.worktree_base_dir);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if !input.is_empty() {
        config.worktree_base_dir = input.to_string();
    }

    // Save configuration
    config.save(repo.root())?;

    println!();
    println!("Configuration saved to {}", config::CONFIG_FILE_NAME);
    println!();
    println!("You can now run 'gwa' to start the TUI.");

    Ok(())
}
