//! A reference of Unreal's everyday console commands and command-line flags.
//!
//! These are the ones people look up again and again: showing stats, changing
//! resolution and view modes, timing, travel, logging, and the flags for
//! launching the editor or a game. Kept as plain data so it can be searched and
//! tested; every entry is something that works in a recent 5.x engine.

pub struct Entry {
    pub group: &'static str,
    /// What to type (a console command) or pass (a command-line flag).
    pub text:  &'static str,
    pub what:  &'static str,
    /// A command-line flag rather than an in-game console command.
    pub flag:  bool,
}

const fn cmd(group: &'static str, text: &'static str, what: &'static str) -> Entry {
    Entry { group, text, what, flag: false }
}
const fn flag(group: &'static str, text: &'static str, what: &'static str) -> Entry {
    Entry { group, text, what, flag: true }
}

pub const ENTRIES: &[Entry] = &[
    // Stats
    cmd("Stats", "stat fps", "Frames per second."),
    cmd("Stats", "stat unit", "Frame, game, draw and GPU time in milliseconds — the first thing to check."),
    cmd("Stats", "stat unitgraph", "The same times as a live graph."),
    cmd("Stats", "stat gpu", "Where the GPU's time goes, pass by pass."),
    cmd("Stats", "stat game", "Game-thread cost by system."),
    cmd("Stats", "stat scenerendering", "Draw calls, visible primitives and other render counts."),
    cmd("Stats", "stat memory", "Memory use by category."),
    cmd("Stats", "stat streaming", "Texture and level streaming."),
    cmd("Stats", "stat none", "Turn all stats off."),
    // Rendering
    cmd("Rendering", "r.SetRes 1920x1080w", "Set the resolution: w windowed, f fullscreen."),
    cmd("Rendering", "r.ScreenPercentage 50", "Render at a fraction of the resolution (100 is native)."),
    cmd("Rendering", "r.VSync 0", "Turn vertical sync off, to see uncapped frame rate."),
    cmd("Rendering", "t.MaxFPS 60", "Cap the frame rate."),
    cmd("Rendering", "viewmode wireframe", "Wireframe. Others: lit, unlit, detaillighting, lightingonly, shadercomplexity."),
    cmd("Rendering", "show collision", "Draw collision shapes. `show` toggles many other flags (bounds, fog, bsp…)."),
    cmd("Rendering", "profilegpu", "Capture one frame's GPU timings."),
    // Debugging
    cmd("Debugging", "showdebug", "Show debug info for the possessed actor. `showdebug ai`, `showdebug animation`… narrow it."),
    cmd("Debugging", "ToggleDebugCamera", "Detach from the player and fly around the level."),
    cmd("Debugging", "slomo 0.2", "Change the game's time speed (1 is normal)."),
    cmd("Debugging", "pause", "Pause and resume the game."),
    cmd("Debugging", "obj list", "List loaded objects. `obj list class=StaticMesh` filters by class."),
    cmd("Debugging", "memreport -full", "Write a memory report to Saved/Profiling."),
    cmd("Debugging", "DumpConsoleCommands", "List every console command and variable the game knows."),
    cmd("Debugging", "Log LogTemp Verbose", "Raise one log category's verbosity. `Log list` shows the categories."),
    // Play & travel
    cmd("Play & travel", "open MapName", "Load a map, leaving the current one."),
    cmd("Play & travel", "servertravel MapName", "Move a listen or dedicated server (and its clients) to a map."),
    cmd("Play & travel", "restartlevel", "Reload the current level."),
    cmd("Play & travel", "quit", "Close the game."),
    cmd("Play & travel", "god", "Make the player invulnerable (cheats must be enabled)."),
    cmd("Play & travel", "fly", "Free flight. `walk` returns to normal movement; `ghost` also ignores collision."),
    // Launch flags
    flag("Launch flags", "-log", "Open the log window alongside the editor or game."),
    flag("Launch flags", "-game", "Run the game from the editor's executable, without packaging."),
    flag("Launch flags", "-windowed", "Start in a window. Pair with -ResX=1280 -ResY=720."),
    flag("Launch flags", "-ResX=1280 -ResY=720", "Window size."),
    flag("Launch flags", "-nosound", "Silence the game."),
    flag("Launch flags", "-nosplash", "Skip the splash screen."),
    flag("Launch flags", "-dx12", "Pick the renderer API. Others: -dx11, -vulkan."),
    flag("Launch flags", "-ExecCmds=\"stat fps,stat unit\"", "Run console commands at startup, comma-separated."),
    flag("Launch flags", "-LogCmds=\"LogTemp Verbose\"", "Set log verbosity at startup."),
    flag("Launch flags", "-nullrhi", "No rendering at all — for servers and automated runs."),
    flag("Launch flags", "-server", "Run as a dedicated server."),
    flag("Launch flags", "-unattended", "Never wait for a dialog: for scripts and builds."),
    flag("Launch flags", "-stdout", "Send the log to standard output."),
    flag("Launch flags", "-ForceLogFlush", "Write every log line immediately, so a crash loses nothing."),
    flag("Launch flags", "-abslog=C:/Temp/run.log", "Write the log to this exact file."),
    flag("Launch flags", "-benchmark -fps=60", "Run at a fixed time step, for repeatable timing."),
];

/// Entries whose text, description or group contain every word typed.
pub fn search(query: &str) -> Vec<&'static Entry> {
    let words: Vec<String> = query.split_whitespace().map(str::to_ascii_lowercase).collect();
    ENTRIES.iter().filter(|e| {
        let hay = format!("{} {} {}", e.group, e.text, e.what).to_ascii_lowercase();
        words.iter().all(|w| hay.contains(w))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_matches_every_word_across_text_group_and_description() {
        assert_eq!(search("").len(), ENTRIES.len());
        assert!(search("stat unit").iter().any(|e| e.text == "stat unit"));
        assert!(search("wireframe").iter().any(|e| e.text.starts_with("viewmode")));
        assert!(search("dedicated").iter().any(|e| e.text == "-server"), "found by description");
        assert!(search("launch flags nosound").len() == 1, "found by group");
        assert!(search("zzzz").is_empty());
    }

    #[test]
    fn entries_are_well_formed() {
        for e in ENTRIES {
            assert!(!e.text.is_empty() && !e.what.is_empty() && !e.group.is_empty());
            assert_eq!(e.flag, e.text.starts_with('-'), "flags start with a dash: {}", e.text);
        }
        // Console commands are unique, so a search never shows duplicates.
        let mut texts: Vec<&str> = ENTRIES.iter().map(|e| e.text).collect();
        texts.sort();
        texts.dedup();
        assert_eq!(texts.len(), ENTRIES.len());
    }
}
