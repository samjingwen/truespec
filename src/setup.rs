use std::{
    collections::HashSet,
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use uuid::Uuid;

const OPENCODE_PLUGIN: &str = include_str!("../.opencode/plugins/truespec/index.ts");
const BINARY_PLACEHOLDER: &str = "__TRUESPEC_INSTALLED_BINARY__";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, ValueEnum)]
pub enum Connector {
    #[value(name = "opencode")]
    OpenCode,
}

impl Connector {
    const ALL: [Self; 1] = [Self::OpenCode];

    fn id(self) -> &'static str {
        match self {
            Self::OpenCode => "opencode",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::OpenCode => "OpenCode",
        }
    }

    fn destination(self) -> Result<PathBuf> {
        match self {
            Self::OpenCode => Ok(config_dir()?.join("opencode/plugins/truespec/index.ts")),
        }
    }

    fn contents(self) -> Result<String> {
        match self {
            Self::OpenCode => {
                let executable = std::env::current_exe()?.canonicalize()?;
                let executable = executable
                    .to_str()
                    .context("TrueSpec executable path is not UTF-8")?;
                let encoded = serde_json::to_string(executable)?;
                Ok(OPENCODE_PLUGIN.replace(BINARY_PLACEHOLDER, &encoded[1..encoded.len() - 1]))
            }
        }
    }

    fn state(self) -> Result<State> {
        let destination = self.destination()?;
        match fs::read_to_string(destination) {
            Ok(existing) if existing == self.contents()? => Ok(State::UpToDate),
            Ok(_) => Ok(State::Different),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(State::NotInstalled),
            Err(error) => Err(error).context("Cannot inspect connector installation"),
        }
    }

    fn install(self, force: bool) -> Result<()> {
        let destination = self.destination()?;
        let source = self.contents()?;
        match fs::read_to_string(&destination) {
            Ok(existing) if existing == source => {
                println!(
                    "{} connector is already up to date: {}",
                    self.label(),
                    destination.display()
                );
                return Ok(());
            }
            Ok(_) if !force => bail!(
                "Refusing to overwrite a different connector file: {}\nRe-run with --force only if you want to replace it.",
                destination.display()
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("Cannot inspect connector installation"),
            Ok(_) => {}
        }
        let directory = destination
            .parent()
            .context("Connector destination has no parent directory")?;
        fs::create_dir_all(directory)
            .with_context(|| format!("Cannot create {}", directory.display()))?;
        let temporary = directory.join(format!(".index.ts.{}.tmp", Uuid::new_v4()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("Cannot stage {}", temporary.display()))?;
        file.write_all(source.as_bytes())?;
        file.sync_all()?;
        if force && destination.exists() {
            #[cfg(windows)]
            fs::remove_file(&destination)?;
        }
        fs::rename(&temporary, &destination)
            .with_context(|| format!("Cannot install {}", destination.display()))?;
        println!(
            "Installed {} connector: {}",
            self.label(),
            destination.display()
        );
        if matches!(self, Self::OpenCode) {
            println!(
                "OpenCode watches this directory. Run /reload if /truespec-review is not visible."
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum State {
    NotInstalled,
    UpToDate,
    Different,
}

impl State {
    fn label(self) -> &'static str {
        match self {
            Self::NotInstalled => "not installed",
            Self::UpToDate => "up to date",
            Self::Different => "different file present",
        }
    }
}

pub fn run(mut selected: Vec<Connector>, all: bool, list: bool, force: bool) -> Result<()> {
    if list {
        print_connectors()?;
        return Ok(());
    }
    if all {
        selected = Connector::ALL.to_vec();
    } else if selected.is_empty() {
        if io::stdin().is_terminal() && io::stdout().is_terminal() {
            selected = prompt()?;
        } else {
            print_connectors()?;
            bail!("No connector selected. Run `truespec setup --connector opencode`.");
        }
    }
    let mut seen = HashSet::new();
    selected.retain(|connector| seen.insert(*connector));
    for connector in selected {
        connector.install(force)?;
    }
    Ok(())
}

fn print_connectors() -> Result<()> {
    println!("Available coding harness connectors:");
    for connector in Connector::ALL {
        println!(
            "  {:<12} {:<16} {}",
            connector.id(),
            connector.label(),
            connector.state()?.label()
        );
    }
    Ok(())
}

fn prompt() -> Result<Vec<Connector>> {
    println!("Select coding harness connectors to install:");
    for (index, connector) in Connector::ALL.iter().enumerate() {
        println!(
            "  {}. [ ] {} ({})",
            index + 1,
            connector.label(),
            connector.state()?.label()
        );
    }
    print!("Selection [1]: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim();
    if answer.is_empty() {
        return Ok(Connector::ALL.to_vec());
    }
    let mut selected = Vec::new();
    for token in answer
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let connector = match token {
            "1" | "opencode" => Connector::OpenCode,
            _ => bail!("Unknown connector selection: {token}"),
        };
        selected.push(connector);
    }
    if selected.is_empty() {
        bail!("Select at least one connector.");
    }
    Ok(selected)
}

fn config_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("APPDATA").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("Cannot find the user configuration directory")?;
    Ok(Path::new(&home).join(".config"))
}
