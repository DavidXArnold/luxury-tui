//! ASCII rendering of the Luxury Yacht mark, reimagined for a terminal.
//!
//! Luxury TUI is a terminal companion to [Luxury Yacht](https://github.com/luxury-yacht/app)
//! by John Jeffers, whose real mark is a captain's-hat badge reading
//! "Luxury Yacht". This is an original ASCII interpretation of that same
//! idea — hat, badge, banner — not a reproduction of any bitmap/vector
//! asset from the upstream project.

/// Full splash logo shown on the context-picker / startup screen.
pub const LOGO_LARGE: &str = r#"
                      _.-'''''-._
                   .-'           '-.
                  /      .-'''-.     \
                 |      (   @   )     |
                  \      '-...-'     /
                   '-._         _.-'
                   .-'` ~  ~  ~ `'-.
                .'                    '.
               /                        \
              |                          |
              |         L u x u r y      |
              |                          |
              |          T u i           |
              |                          |
               \                        /
                '.                    .'
                 '-._              _.-'
                 .-'`              `'-.
                |   LUXURY-TUI  APP    |
                 '--------------------'
"#;

/// Compact one-line wordmark for status bars / headers.
pub const LOGO_SMALL: &str = "\u{2693} Luxury TUI";

/// Small badge used in the help screen.
pub const LOGO_BADGE: &str = r#"
           .-''-.
          (  @  )
        .--` ~ `--.
       /           \
       |  Luxury   |
       |    Tui    |
       \           /
        '-.._____.-'
"#;
