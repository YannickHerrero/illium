use illium::layout::{Axis, Rect, Splits, fibonacci};

#[test]
fn default_shortcuts_and_editor_labels() {
    use illium::{command::Command, config::Keys, keybindings, keyboard};
    let keys: Keys = toml::from_str(keybindings::DEFAULTS).unwrap();
    let bindings = keyboard::parse(&keys).unwrap();
    for (key, axis, delta, label) in [
        ('U', Axis::Width, -5, "Decrease window width by 5%"),
        ('P', Axis::Width, 5, "Increase window width by 5%"),
        ('I', Axis::Height, -5, "Decrease window height by 5%"),
        ('O', Axis::Height, 5, "Increase window height by 5%"),
    ] {
        let binding = bindings
            .iter()
            .find(|b| b.key == key as u32 && b.modifiers == keyboard::ALT)
            .unwrap();
        assert_eq!(binding.command, Command::Resize { axis, delta });
        assert_eq!(
            keybindings::describe(&keys.keybindings[&format!("Alt+{key}")]),
            label
        );
    }
    // An existing custom file is not silently granted new shortcuts.
    let rows = keybindings::rows(
        keybindings::DEFAULTS,
        "[keybindings]\n'Alt+Q'='window close'\n",
    );
    let resize: Vec<_> = rows
        .iter()
        .filter(|r| r.command.starts_with("window resize "))
        .collect();
    assert_eq!(resize.len(), 4);
    assert!(resize.iter().all(|r| r.chord.is_none()));
}

fn area() -> Rect {
    Rect {
        x: -1000,
        y: 20,
        w: 1600,
        h: 1000,
    }
}
fn splits(count: usize) -> Splits {
    let mut splits = Splits::default();
    splits.sync((0..count).map(|i| (i as isize, i)).collect());
    splits
}
#[test]
fn defaults_match_existing_layout() {
    for n in 0..20 {
        for gap in [0, 1, 6, 20] {
            assert_eq!(
                splits(n).layout(area(), n, gap, 6),
                fibonacci(area(), n, gap, 6)
            );
        }
    }
}
#[test]
fn closest_parent_and_sign_for_every_position() {
    for count in 2..7 {
        for index in 0..count {
            for axis in [Axis::Width, Axis::Height] {
                for delta in [-5, 5] {
                    let mut s = splits(count);
                    let before = s.layout(area(), count, 6, 6);
                    let changed = s.resize(index, axis, delta);
                    let after = s.layout(area(), count, 6, 6);
                    if axis == Axis::Height && (index == 0 || count == 2) {
                        assert!(!changed);
                        assert_eq!(before, after);
                        continue;
                    }
                    assert!(changed);
                    let dimension = |r: Rect| if axis == Axis::Width { r.w } else { r.h };
                    assert_eq!(
                        (dimension(after[index]) - dimension(before[index])).signum(),
                        delta.signum()
                    );
                    let split = (0..(count - 1).min(index + 1))
                        .rev()
                        .find(|i| (*i % 2 == 0) == (axis == Axis::Width))
                        .unwrap();
                    assert_eq!(&before[..split], &after[..split]);
                    assert!(s.resize(index, axis, -delta));
                    assert_eq!(before, s.layout(area(), count, 6, 6));
                }
            }
        }
    }
}
#[test]
fn limits_and_degenerate_cases() {
    assert!(!splits(0).resize(0, Axis::Width, 5));
    assert!(!splits(1).resize(0, Axis::Width, 5));
    let mut s = splits(2);
    assert!(!s.resize(2, Axis::Width, 5));
    assert!(!s.resize(0, Axis::Width, i32::MIN));
    for _ in 0..100 {
        s.resize(0, Axis::Width, 5);
    }
    assert!(!s.resize(0, Axis::Width, 5));
    let rs = s.layout(area(), 2, 6, 6);
    assert_eq!(rs[0].x + rs[0].w + 6, rs[1].x);
    assert_eq!(rs[1].x + rs[1].w, area().x + area().w - 6);
    for size in 1..20 {
        let rs = s.layout(
            Rect {
                x: 0,
                y: 0,
                w: size,
                h: size,
            },
            2,
            100,
            100,
        );
        assert!(
            rs.iter()
                .all(|r| r.w > 0 && r.h > 0 && r.x + r.w <= size && r.y + r.h <= size)
        );
    }
}
#[test]
fn membership_order_generation_and_workspace_isolation() {
    let mut model = illium::model::Model::new();
    model.splits[0].sync(vec![(1, 1), (2, 2)]);
    model.splits[1].sync(vec![(3, 3), (4, 4)]);
    model.splits[0].resize(0, Axis::Width, 5);
    let resized = model.splits[0].layout(area(), 2, 6, 6);
    model.splits[0].sync(vec![(2, 2), (1, 1)]);
    model.switch(2);
    model.switch(1);
    assert_eq!(model.splits[0].layout(area(), 2, 6, 6), resized);
    assert_eq!(
        model.splits[1].layout(area(), 2, 6, 6),
        fibonacci(area(), 2, 6, 6)
    );
    // A reused HWND is a different member, even at the same count.
    model.splits[0].sync(vec![(1, 9), (2, 2)]);
    assert_eq!(
        model.splits[0].layout(area(), 2, 6, 6),
        fibonacci(area(), 2, 6, 6)
    );
    model.splits[0].resize(0, Axis::Width, 5);
    model.splits[0].sync(vec![(1, 9)]);
    model.splits[0].sync(vec![(1, 9), (2, 2)]);
    assert_eq!(
        model.splits[0].layout(area(), 2, 6, 6),
        fibonacci(area(), 2, 6, 6)
    );
}
