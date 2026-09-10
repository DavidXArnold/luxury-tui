//! ASCII rendering of the Luxury Yacht mark, reimagined for a terminal.
//!
//! Luxury TUI is a terminal companion to [Luxury Yacht](https://github.com/luxury-yacht/app)
//! by John Jeffers, whose real mark is a captain's-hat badge reading
//! "Luxury Yacht". This is an original ASCII interpretation of that same
//! idea — hat, badge, banner — not a reproduction of any bitmap/vector
//! asset from the upstream project.

/// Full splash logo shown on the context-picker / startup screen — an
/// ASCII-art rendering of the badge, supplied directly rather than
/// hand-drawn.
pub const LOGO_LARGE: &str = r#"
                        :--:--------:
                   ::----::        ::---::
              ::----:          :=-=-     :---
           :---:             :++*:++*-      -=
          --:                -#=----*+       =: :
       : --                  :++=--=+-      =-
          --    ::---=========-==-==-------=:
           :-+*+:-+-:------=============--=+
             +#*==++*+++--++++*+************+=:
             =+=-:::-+*+--= :=::--=************:
           ==-        :-=++++**+**********++=--==
          =-                ::--------::::      -=
         +-     :  :::                           :=
       :+-     :  :: :                            :=
      :+:     : ::::: ::: ::  :::: :: :: : :: ::: ::+:
     :+:       :   :  :: :::: :::  :  :::  :  ::    :=:
  : :+: : ::   :  :: ::  ::::  :: ::  ::: ::  :  :   :=:
    +:              ::::: :                 ::: ::::  :=
    =-            :  :      ::    :   :: :   : :      -=
     -=:         :  : : :  ::  : :    : :  :::      :=-
      :+:          :::  :  :  : :  : :  :          :+:
        =-       :  :  :   : :::  :::  : ::       -=
         ==      :    :                          ==
          -=:     :::::::::::::::::::::::      :=-
           -+=+++++*+++*+++****+******++++++++=+-
          =*##**##+*+*+*+++*+**+*+**#*++++*#*****=
          :+===*=----:-----::-:-----:------=*===+
               :=--------------------------=:
                 ::::::::::::::::::::::::::
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
