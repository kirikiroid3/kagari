//! The std ADTs the engine builds or reads, with their exact variant and field order (the order
//! of the `.kis` declarations, which the emitter keeps). `values/tests.rs` checks this table
//! against a compiled program.

/// A std struct or enum. A struct is one variant named like the struct.
pub(crate) struct AdtDecl {
    pub path: &'static str,
    pub is_enum: bool,
    pub variants: &'static [VariantDecl],
}

/// A variant; positional fields are `None`.
pub(crate) struct VariantDecl {
    pub name: &'static str,
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the drift test"))]
    pub fields: &'static [Option<&'static str>],
}

macro_rules! field {
    (_) => {
        None
    };
    ($name:ident) => {
        Some(stringify!($name))
    };
}

macro_rules! std_adts {
    ($($ty:ident = $path:literal, $is_enum:literal, [$($var:ident ($($field:tt),*)),* $(,)?];)*) => {
        /// A std type, by the last segment of its path; the discriminant indexes `STD_ADTS`.
        #[repr(usize)]
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub(crate) enum StdTy { $($ty),* }

        impl StdTy {
            pub const COUNT: usize = [$(StdTy::$ty),*].len();
        }

        pub(crate) static STD_ADTS: [AdtDecl; StdTy::COUNT] = [$(
            AdtDecl {
                path: $path,
                is_enum: $is_enum,
                variants: &[$(VariantDecl { name: stringify!($var), fields: &[$(field!($field)),*] }),*],
            }
        ),*];
    };
}

std_adts! {
    Option = "std::Option", true, [None(), Some(_)];
    Result = "std::Result", true, [Ok(_), Err(_)];
    Duration = "std::time::Duration", false, [Duration(ms)];

    EntityId = "std::screen::EntityId", false, [EntityId(_)];
    Part = "std::screen::Part", false, [Part(_)];
    Point = "std::screen::Point", false, [Point(x, y)];
    Offset = "std::screen::Offset", false, [Offset(dx, dy)];
    Size = "std::screen::Size", false, [Size(w, h)];
    Insets = "std::screen::Insets", false, [Insets(top, right, bottom, left)];
    Anchor = "std::screen::Anchor", false, [Anchor(x, y)];
    Angle = "std::screen::Angle", false, [Angle(degrees)];
    Color = "std::screen::Color", false, [Color(r, g, b, a)];
    Plane = "std::screen::Plane", true, [Background(), Stage(), Front(), OverWindow()];
    Visibility = "std::screen::Visibility", true, [Shown(), Hidden()];
    Flip = "std::screen::Flip", true, [None(), Horizontal(), Vertical(), Both()];
    VAlign = "std::screen::VAlign", true, [Top(), Middle(), Bottom()];
    Pose = "std::screen::Pose", false, [Pose(position, scale, rotation)];
    Movie = "std::screen::Movie", false, [Movie(video, looping)];

    Easing = "std::screen::anim::Easing", true, [
        Linear(), InQuad(), OutQuad(), InOutQuad(), InCubic(), OutCubic(), InOutCubic(),
        InQuart(), OutQuart(), InOutQuart(), InQuint(), OutQuint(), InOutQuint(), InSine(),
        OutSine(), InOutSine(), InExpo(), OutExpo(), InOutExpo(), InCirc(), OutCirc(),
        InOutCirc(), InBack(), OutBack(), InOutBack(), InElastic(), OutElastic(),
        InOutElastic(), InBounce(), OutBounce(), InOutBounce(), CubicBezier(_, _, _, _),
        Steps(_),
    ];
    Tween = "std::screen::anim::Tween", false, [Tween(duration, easing, delay)];
    Direction = "std::screen::anim::Direction", true, [Left(), Right(), Up(), Down()];
    TransitionKind = "std::screen::anim::TransitionKind", true, [
        Cut(), Crossfade(), Rule(mask, softness, reverse), Wipe(_), Blinds(direction, count),
        ThroughColor(color, hold), Masked(mask, softness, reverse),
    ];
    Transition = "std::screen::anim::Transition", false, [Transition(kind, duration, easing, delay)];
    MotionKey = "std::screen::anim::MotionKey", false, [MotionKey(at, offset, scale, rotation, easing)];
    Motion = "std::screen::anim::Motion", false, [Motion(keys, repeat, delay)];

    Loop = "std::screen::audio::Loop", false, [Loop(audio, loop_start, loop_end)];
    Sound = "std::screen::audio::Sound", false, [Sound(audio, volume, start, end, delay)];
    AudioFade = "std::screen::audio::AudioFade", false, [AudioFade(fade_out, fade_in, overlap)];

    Filter = "std::screen::shader::Filter", false, [Filter(shader, args)];
    Mix = "std::screen::shader::Mix", false, [Mix(shader, args)];
    Mask = "std::screen::shader::Mask", false, [Mask(shader, args)];
    ShaderPaint = "std::screen::shader::Paint", false, [Paint(texture, args, size, mask)];
    LocalMotionBehavior = "std::screen::shader::LocalMotionBehavior", true, [
        Periodic(after, period), Stall(after),
    ];
    LocalMotion = "std::screen::shader::LocalMotion", false, [LocalMotion(shader, args, behavior)];
    TransitionalMotion = "std::screen::shader::TransitionalMotion", false, [TransitionalMotion(
        shader, args, duration, easing, delay
    )];

    Mark = "std::narrative::text::Mark", false, [Mark(_)];
    Weight = "std::narrative::text::Weight", true, [Regular(), Bold()];
    Emphasis = "std::narrative::text::Emphasis", true, [Styled(), Dots()];
    Reveal = "std::narrative::text::Reveal", true, [Player(), PerChar(_), Scaled(_), Instant()];
    Pause = "std::narrative::text::Pause", true, [Time(_), Click()];
    Outline = "std::narrative::text::Outline", false, [Outline(color, width)];
    Shadow = "std::narrative::text::Shadow", false, [Shadow(color, offset, blur)];
    TextEffect = "std::narrative::text::TextEffect", false, [TextEffect(
        color, size, weight, italic, font, outline, shadow, emphasis, reveal, pause, mark
    )];
    TextAlign = "std::narrative::text::TextAlign", true, [Start(), Center(), End()];
    EmphasisStyle = "std::narrative::text::EmphasisStyle", true, [Dots(), Bold(), Italic()];
    TextStyle = "std::narrative::text::TextStyle", false, [TextStyle(
        font, size, color, line_height, letter_spacing, align, outline, shadow, ruby_size,
        emphasis, hanging_indent, vertical
    )];

    Say = "std::narrative::Say", false, [Say(who, voice, render, text, history)];
    Choice = "std::narrative::Choice", false, [Choice(id, render, arms)];
    ChoiceArm = "std::narrative::ChoiceArm", false, [ChoiceArm(text, shown, value)];
    Line = "std::narrative::Line", false, [Line(who, text, voice, read)];
    MenuArm = "std::narrative::MenuArm", false, [MenuArm(text, arm)];
    Menu = "std::narrative::Menu", false, [Menu(arms, align)];
    HudState = "std::narrative::HudState", false, [HudState(text_window, quick_menu, side_image)];
    Entry = "std::narrative::Entry", true, [Line(_), Chosen(_)];
    PlayMode = "std::narrative::PlayMode", true, [Normal(), Auto(), Skip()];

    Volume = "std::system::Volume", false, [Volume(level, muted)];
    Settings = "std::system::Settings", false, [Settings(
        text_speed, auto_delay, auto_delay_per_char, skip_unread, master, music, sound, voice,
        voice_sustain, wait_for_voice, window_opacity, fullscreen, animations
    )];
    RootPage = "std::system::RootPage", false, [RootPage(_)];
    Overlay = "std::system::Overlay", false, [Overlay(_)];
    GameOverlay = "std::system::GameOverlay", false, [GameOverlay(_)];
    Dialog = "std::system::Dialog", false, [Dialog(_)];
    System = "std::system::System", false, [System(
        title, resolution, start, game, save_slots, auto_slots, backlog, font, text, settings,
        bindings
    )];
    Key = "std::system::Key", true, [
        Enter(), Escape(), Space(), Backspace(), Tab(), Up(), Down(), Left(), Right(), PageUp(),
        PageDown(), Home(), End(), Shift(), Control(), Alt(), F1(), F2(), F3(), F4(), F5(),
        F6(), F7(), F8(), F9(), F10(), F11(), F12(), A(), B(), C(), D(), E(), F(), G(), H(),
        I(), J(), K(), L(), M(), N(), O(), P(), Q(), R(), S(), T(), U(), V(), W(), X(), Y(),
        Z(), Digit0(), Digit1(), Digit2(), Digit3(), Digit4(), Digit5(), Digit6(), Digit7(),
        Digit8(), Digit9(),
    ];
    MouseButton = "std::system::MouseButton", true, [
        Left(), Right(), Middle(), Back(), Forward(), WheelUp(), WheelDown(),
    ];
    PadButton = "std::system::PadButton", true, [
        South(), East(), West(), North(), LeftShoulder(), RightShoulder(), LeftTrigger(),
        RightTrigger(), Start(), Select(), LeftStick(), RightStick(), DpadUp(), DpadDown(),
        DpadLeft(), DpadRight(),
    ];
    Input = "std::system::Input", true, [Key(_), Mouse(_), Pad(_)];
    Action = "std::system::Action", true, [
        Advance(), HoldSkip(), ToggleSkip(), ToggleAuto(), HideWindow(), QuickSave(),
        QuickLoad(), Back(), Open(_), OpenInGame(_),
    ];
    Binding = "std::system::Binding", false, [Binding(input, action)];

    SaveSlot = "std::system::saves::SaveSlot", true, [Manual(_), Quick(), Auto(_)];
    Timestamp = "std::system::saves::Timestamp", false, [Timestamp(year, month, day, hour, minute, second)];
    SaveInfo = "std::system::saves::SaveInfo", false, [SaveInfo(time, who, text)];

    Align = "std::ui::Align", true, [Start(), Center(), End(), Stretch(), SpaceBetween()];
    Fit = "std::ui::Fit", true, [Stretch(), Contain(), Cover(), Natural()];
    TextProps = "std::ui::TextProps", false, [TextProps(style, content)];
    ImageProps = "std::ui::ImageProps", false, [ImageProps(fit, source)];
    GameScreenProps = "std::ui::GameScreenProps", false, [GameScreenProps(game)];
    ButtonProps = "std::ui::ButtonProps", false, [ButtonProps(on_click, disabled, style, children)];
    SliderProps = "std::ui::SliderProps", false, [SliderProps(value, on_change, step, disabled, style)];
    ToggleProps = "std::ui::ToggleProps", false, [ToggleProps(value, on_change, disabled, style, children)];
    SelectOption = "std::ui::SelectOption", false, [SelectOption(value, label)];
    SelectProps = "std::ui::SelectProps", false, [SelectProps(value, on_change, disabled, style, options)];

    Paint = "std::ui::style::Paint", true, [None(), Color(_), Image(_), NineSlice(image, insets)];
    Look = "std::ui::style::Look", false, [Look(background, text)];
    ButtonStyle = "std::ui::style::ButtonStyle", false, [ButtonStyle(
        idle, hover, focus, pressed, disabled, change, hover_sound, click_sound
    )];
    SliderStyle = "std::ui::style::SliderStyle", false, [SliderStyle(track, fill, thumb, thumb_size)];
}
