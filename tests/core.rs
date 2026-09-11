use winarchy::{
    command::{Command, Direction},
    config::{Config, Rule},
    layout::{Rect, fibonacci, neighbor},
    model::{Client, Model},
};
fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}
#[test]
fn fibonacci_two() {
    assert_eq!(
        fibonacci(rect(0, 0, 100, 80), 2, 0, 0),
        vec![rect(0, 0, 50, 80), rect(50, 0, 50, 80)]
    );
}
#[test]
fn fibonacci_five() {
    assert_eq!(
        fibonacci(rect(0, 0, 128, 128), 5, 0, 0),
        vec![
            rect(0, 0, 64, 128),
            rect(64, 0, 64, 64),
            rect(64, 64, 32, 64),
            rect(96, 64, 32, 32),
            rect(96, 96, 32, 32)
        ]
    );
}
#[test]
fn fibonacci_nonoverlapping() {
    for count in 1..=12 {
        for gap in [0, 1, 6, 20] {
            let rs = fibonacci(rect(-1920, -200, 3840, 2160), count, gap, 6);
            for (i, a) in rs.iter().enumerate() {
                for b in &rs[i + 1..] {
                    assert!(
                        a.x + a.w <= b.x
                            || b.x + b.w <= a.x
                            || a.y + a.h <= b.y
                            || b.y + b.h <= a.y
                    );
                }
            }
        }
    }
}
#[test]
fn fibonacci_area_conserved_without_gaps() {
    for n in 1..=12 {
        let rs = fibonacci(rect(0, 0, 2048, 1024), n, 0, 0);
        assert_eq!(rs.iter().map(|r| r.w * r.h).sum::<i32>(), 2048 * 1024);
    }
}
#[test]
fn fibonacci_odd_rounding() {
    let rs = fibonacci(rect(0, 0, 101, 81), 3, 3, 2);
    assert_eq!(rs[0], rect(2, 2, 47, 77));
    assert_eq!(rs[1], rect(52, 2, 47, 37));
    assert_eq!(rs[2], rect(52, 42, 47, 37));
}
#[test]
fn directional_axes() {
    let rs = vec![
        (1, rect(0, 0, 10, 10)),
        (2, rect(-20, 0, 10, 10)),
        (3, rect(20, 0, 10, 10)),
        (4, rect(0, -20, 10, 10)),
        (5, rect(0, 20, 10, 10)),
    ];
    for (d, id) in [
        (Direction::Left, 2),
        (Direction::Right, 3),
        (Direction::Up, 4),
        (Direction::Down, 5),
    ] {
        assert_eq!(neighbor(&rs, 1, d), Some(id));
    }
}
#[test]
fn directional_ties_stable() {
    let rs = vec![
        (1, rect(0, 0, 10, 10)),
        (9, rect(20, -10, 10, 10)),
        (2, rect(20, 10, 10, 10)),
    ];
    assert_eq!(neighbor(&rs, 1, Direction::Right), Some(2));
    assert_eq!(neighbor(&rs, 99, Direction::Right), None);
}
#[test]
fn direction_reorder_changes_geometry() {
    let mut m = Model::new();
    for id in 1..=4 {
        m.clients.push(Client {
            id,
            workspace: 1,
            floating: false,
            fullscreen: false,
            restore: Rect::default(),
        });
    }
    let before = fibonacci(rect(0, 0, 1000, 800), 4, 6, 6);
    let rs = m
        .clients
        .iter()
        .map(|c| c.id)
        .zip(before.clone())
        .collect::<Vec<_>>();
    let target = neighbor(&rs, 1, Direction::Right).unwrap();
    m.swap(1, target);
    let index = m.clients.iter().position(|c| c.id == 1).unwrap();
    assert_ne!(index, 0);
    assert_ne!(before[index], before[0]);
}
#[test]
fn workspace_transition_matrix() {
    for a in 1..=9 {
        for b in 1..=9 {
            let mut m = Model::new();
            m.switch(a);
            let recent = m.recent;
            m.switch(b);
            assert_eq!(m.active, b);
            assert_eq!(m.recent, if a == b { recent } else { a });
        }
    }
}
#[test]
fn workspace_invalid_is_noop() {
    let mut m = Model::new();
    m.switch(0);
    m.switch(10);
    assert_eq!(m.active, 1);
    assert_eq!(m.next(), 1);
}
#[test]
fn workspace_move_preserves_uniqueness() {
    let mut m = Model::new();
    m.clients.push(Client {
        id: 42,
        workspace: 1,
        floating: true,
        fullscreen: false,
        restore: Rect::default(),
    });
    for n in 1..=9 {
        m.move_to(42, n, false);
        assert_eq!(m.active, 1);
        assert_eq!(m.clients.len(), 1);
        assert_eq!(m.clients[0].workspace, n);
        assert!(m.clients[0].floating);
    }
}
#[test]
fn occupied_wraps() {
    let mut m = Model::new();
    m.clients.push(Client {
        id: 1,
        workspace: 2,
        floating: false,
        fullscreen: false,
        restore: Rect::default(),
    });
    m.switch(9);
    assert_eq!(m.next(), 2);
}
#[test]
fn all_command_variants() {
    for s in [
        "workspace next",
        "workspace next-active",
        "workspace recent",
        "window close",
        "window focus up",
        "window move down",
        "window move-workspace 1",
        "window move-workspace 9 --follow",
        "window set-tiling",
        "window toggle-float",
        "window toggle-fullscreen",
        "spawn terminal",
        "launcher toggle",
        "config reload",
        "theme set my-theme",
        "explorer stop",
        "explorer start",
        "quit",
        "status",
    ] {
        assert!(s.parse::<Command>().is_ok(), "{s}");
    }
}
#[test]
fn command_rejects_extra_arguments() {
    for s in [
        "spawn app extra",
        "window close now",
        "workspace 2 --follow",
        "window move-workspace 2 --bad",
        "theme set",
        "explorer restart",
    ] {
        assert!(s.parse::<Command>().is_err(), "{s}");
    }
}
#[test]
fn rule_all_fields_required() {
    let r = Rule {
        executable: Some("foo".into()),
        class: Some("widget".into()),
        title: Some("doc".into()),
        ..Rule::default()
    };
    assert!(r.matches("C:\\Foo.exe", "WidgetClass", "DOCUMENT"));
    assert!(!r.matches("foo", "widget", "other"));
    assert!(!r.matches("bar", "widget", "doc"));
}
#[test]
fn semantic_invalid_config() {
    let p = std::env::temp_dir().join(format!("winarchy-semantic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    Config::install(&p).unwrap();
    for (name, invalid) in [
        (
            "wm.toml",
            "workspaces=8\nlayout='fibonacci'\ngap=6\nouter_gap=6\nfocus_follows_mouse=false",
        ),
        ("keybindings.toml", "[keybindings]\n'Alt+A+B'='quit'"),
        ("rules.toml", "[[rules]]\nworkspace=10"),
        ("winarchy.toml", "theme='../bad'"),
        ("themes/catppuccin-mocha.toml", "name='broken'"),
    ] {
        let file = p.join(name);
        let old = std::fs::read(&file).unwrap();
        std::fs::write(&file, invalid).unwrap();
        assert!(Config::load(&p).is_err(), "{name}");
        std::fs::write(file, old).unwrap();
        assert!(Config::load(&p).is_ok());
    }
    std::fs::remove_dir_all(p).unwrap();
}
