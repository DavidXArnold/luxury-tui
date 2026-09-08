//! ASCII rendering of the Luxury Yacht mark, reimagined for a terminal.
//!
//! Luxury TUI is a terminal companion to [Luxury Yacht](https://github.com/luxury-yacht/app)
//! by John Jeffers. This is an original ASCII interpretation of a yacht, not a
//! reproduction of any bitmap/vector asset from the upstream project.

/// Full splash logo shown on the context-picker / startup screen.
pub const LOGO_LARGE: &str = r#"
                                        |>
                                       /||\
                                      / || \
                                     /  ||  \
                                    /   ||   \
                                   /    ||    \
                                  /_____||_____\
                                  \            /
                           _.--~~~~._  L  _.~~~~--._
                       _.-'          `--'          `-._
                   _.-'                                `-._
               _.-'      L  U  X  U  R  Y     T  U  I       `-._
        ~~~~~~'~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
              `~._                                              _.~'
                  `~._                                      _.~'
                      `~._                              _.~'
                          `~---___              ___---~'
                                  ~~~~~~~~~~~~~~
"#;

/// Compact one-line wordmark for status bars / headers.
pub const LOGO_SMALL: &str = "\u{26f5} Luxury TUI";

/// Small badge used in the command palette / help screen footer.
pub const LOGO_BADGE: &str = r#"
   __
  /  \___   Luxury TUI
  \___/ )   a terminal yacht for your clusters
      \|
"#;
