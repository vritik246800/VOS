#[derive(Debug, Clone)]
pub enum Command {
    Quit,
    Save,
    OpenFile(String),
    SplitH,
    SplitV,
    SetTheme(String),
    ShowHelp,
    ShowTerminal,
    ShowProcessViewer,
    ShowFavorites,
    ShowNotes,
    ShowWeather,
    ShowCalendar,
    ShowSsh,
    OpenThemeSwitcher,
    PluginList,
    Git(GitCommand),
    Unknown(String),
}

#[derive(Debug, Clone)]
pub enum GitCommand {
    Status,
    Add(String),
    Commit(String),
    Pull,
    Push,
}

pub struct CommandParser;

impl CommandParser {
    pub fn parse(input: &str) -> Command {
        let input = input.trim();
        if input.is_empty() {
            return Command::Unknown(String::new());
        }

        let mut parts = input.splitn(3, ' ');
        let cmd = parts.next().unwrap_or("");
        let arg1 = parts.next().unwrap_or("").trim();
        let arg2 = parts.next().unwrap_or("").trim();

        match cmd {
            "q" | "quit" => Command::Quit,
            "wq" => Command::Quit,
            "w" | "save" => Command::Save,
            "open" if !arg1.is_empty() => Command::OpenFile(arg1.to_string()),
            "split" => match arg1 {
                "h" | "horizontal" => Command::SplitH,
                "v" | "vertical" => Command::SplitV,
                _ => Command::SplitH,
            },
            "theme" if arg1.is_empty() => Command::OpenThemeSwitcher,
            "theme" if !arg1.is_empty() => Command::SetTheme(arg1.to_string()),
            "help" => Command::ShowHelp,
            "terminal" | "term" => Command::ShowTerminal,
            "git" => match arg1 {
                "status" | "" => Command::Git(GitCommand::Status),
                "add" => Command::Git(GitCommand::Add(arg2.to_string())),
                "commit" => Command::Git(GitCommand::Commit(arg2.to_string())),
                "pull" => Command::Git(GitCommand::Pull),
                "push" => Command::Git(GitCommand::Push),
                _ => Command::Unknown(input.to_string()),
            },
            "ps" | "proc" | "processes" | "sys" | "sysmon" | "monitor" => {
                Command::ShowProcessViewer
            }
            "fav" | "favorites" | "favourites" => Command::ShowFavorites,
            "notes" | "note" => Command::ShowNotes,
            "weather" | "wttr" => Command::ShowWeather,
            "calendar" | "cal" => Command::ShowCalendar,
            "ssh" => Command::ShowSsh,
            "plugin" => Command::PluginList,
            _ => Command::Unknown(input.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quit() {
        assert!(matches!(CommandParser::parse("q"), Command::Quit));
        assert!(matches!(CommandParser::parse("quit"), Command::Quit));
        assert!(matches!(CommandParser::parse("wq"), Command::Quit));
    }

    #[test]
    fn test_open() {
        assert!(matches!(
            CommandParser::parse("open /tmp/foo.txt"),
            Command::OpenFile(_)
        ));
    }

    #[test]
    fn test_ssh() {
        assert!(matches!(CommandParser::parse("ssh"), Command::ShowSsh));
    }

    #[test]
    fn test_git() {
        assert!(matches!(
            CommandParser::parse("git status"),
            Command::Git(GitCommand::Status)
        ));
        assert!(matches!(
            CommandParser::parse("git pull"),
            Command::Git(GitCommand::Pull)
        ));
    }
}
