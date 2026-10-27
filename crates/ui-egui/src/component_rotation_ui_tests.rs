use super::component_scale_ui_tests::{fixture, path};
use super::*;

fn rotate_drag(a: &mut PhotocraftApp) {
    // Outside the bottom-right corner, turn 90 degrees around the component-local centre.
    pointer(a, ToolEvent::Down { x: 200.0, y: 160.0, pressure: 1.0 }, egui::Modifiers::NONE);
    pointer(a, ToolEvent::Move { x: 90.0, y: 170.0, pressure: 1.0 }, egui::Modifiers::NONE);
    pointer(a, ToolEvent::Up { x: 90.0, y: 170.0 }, egui::Modifiers::NONE);
}
#[test]
fn component_rotation_ui_tests_existing_rotation_drag_previews_and_commits_only_component() {
    for name in ["work", "One"] {
        let mut a = fixture(name);
        let original = path(&a, name);
        let other = path(&a, "Two");
        let ctx = egui::Context::default();
        crate::menus::invoke(&mut a, &ctx, "edit.freeTransform", json!({})).unwrap();
        assert_eq!(a.ui.transform.as_ref().unwrap().rect, [100.0, 80.0, 180.0, 140.0]);
        assert_eq!(a.ui.transform.as_ref().unwrap().pivot, [140.0, 110.0]);
        assert!(rotate_cursor_dir(&a, [190.0, 150.0]).is_some());
        rotate_drag(&mut a);
        let t = a.ui.transform.as_ref().unwrap();
        assert!((rotation_degrees(t.quad) - 90.0).abs() < 1e-8);
        assert_eq!(path(&a, name), original);
        commit(&mut a);
        assert!(a.ui.transform.is_none(), "{}", a.ui.status);
        let after = path(&a, name);
        assert_ne!(after.subpaths[1], original.subpaths[1]);
        assert_eq!(after.subpaths[0], original.subpaths[0]);
        assert_eq!(after.subpaths[2], original.subpaths[2]);
        assert_eq!(path(&a, "Two"), other);
    }
}
#[test]
fn component_rotation_ui_tests_existing_rotate_menu_mode_is_available() {
    let mut a = fixture("One");
    let ctx = egui::Context::default();
    assert!(!component_restricts(&a, "edit.transform.rotate"));
    crate::menus::invoke(&mut a, &ctx, "edit.transform.rotate", json!({})).unwrap();
    assert_eq!(a.ui.transform.as_ref().unwrap().mode, TransformMode::Rotate);
}

#[test]
fn component_rotation_ui_tests_combined_controls_commit_one_matrix_with_no_mixed_parameters() {
    for name in ["work", "One"] {
        for rotate_first in [false, true] {
            let mut a = fixture(name);
            let before = path(&a, name);
            let other = path(&a, if name == "work" { "One" } else { "work" });
            let history = a.session.active().unwrap().history.past_len();
            begin(&mut a, &egui::Context::default()).unwrap();
            if !rotate_first {
                scale_about_pivot(&mut a, 1.5, 0.75);
            }
            rotate_drag(&mut a);
            if rotate_first {
                scale_about_pivot(&mut a, 1.5, 0.75);
            }
            let q = a.ui.transform.as_ref().unwrap().quad;
            let end = [q[0][0] + (q[2][0] - q[0][0]) * 1.2, q[0][1] + (q[2][1] - q[0][1]) * 1.2];
            pointer(&mut a, ToolEvent::Down { x: q[2][0], y: q[2][1], pressure: 1.0 }, egui::Modifiers::SHIFT);
            pointer(&mut a, ToolEvent::Up { x: end[0], y: end[1] }, egui::Modifiers::SHIFT);
            let t = a.ui.transform.as_ref().unwrap();
            let matrix = component_matrix(t).unwrap();
            let h = Homography::rect_to_quad(t.rect, t.quad).unwrap();
            let expected = before.transform(&matrix);
            for k in &before.subpaths[1].knots {
                for p in [k.anchor, k.in_ctrl, k.out_ctrl] {
                    let (x, y) = h.apply(p.x, p.y);
                    assert!((x - (matrix.m[0] * p.x + matrix.m[2] * p.y + matrix.m[4])).abs() < 1e-8);
                    assert!((y - (matrix.m[1] * p.x + matrix.m[3] * p.y + matrix.m[5])).abs() < 1e-8);
                }
            }
            assert_eq!(path(&a, name), before);
            commit(&mut a);
            assert!(a.ui.transform.is_none(), "{}", a.ui.status);
            let (id, p) = a.session.journal.last().unwrap();
            assert_eq!(id, "path.transform");
            assert!(p.get("matrix").is_some());
            for key in ["angle", "scaleX", "scaleY", "quad", "warp"] {
                assert!(p.get(key).is_none(), "{key}");
            }
            let after = path(&a, name);
            assert_eq!(after.subpaths[1], expected.subpaths[1]);
            assert_eq!(after.subpaths[0], before.subpaths[0]);
            assert_eq!(after.subpaths[2], before.subpaths[2]);
            assert_eq!(path(&a, if name == "work" { "One" } else { "work" }), other);
            assert_eq!(a.session.active().unwrap().history.past_len(), history + 1);
            for _ in 0..3 {
                a.run("edit.undo", json!({})).unwrap();
                assert_eq!(path(&a, name), before);
                a.run("edit.redo", json!({})).unwrap();
                assert_eq!(path(&a, name), after);
            }
        }
    }
}
#[test]
fn component_rotation_ui_tests_angle_control_math_uses_moved_reference_point_and_all_handles() {
    for name in ["work", "One"] {
        let mut a = fixture(name);
        let before = path(&a, name);
        begin(&mut a, &egui::Context::default()).unwrap();
        pointer(&mut a, ToolEvent::Down { x: 90.0, y: 50.0, pressure: 1.0 }, egui::Modifiers::ALT);
        pointer(&mut a, ToolEvent::Up { x: 90.0, y: 50.0 }, egui::Modifiers::ALT);
        assert_eq!(a.ui.transform.as_ref().unwrap().pivot, [90.0, 50.0]);
        rotate_about_pivot(&mut a, std::f64::consts::FRAC_PI_2);
        commit(&mut a);
        assert!(a.ui.transform.is_none(), "{}", a.ui.status);
        let after = path(&a, name);
        for (old, new) in before.subpaths[1].knots.iter().zip(&after.subpaths[1].knots) {
            assert_eq!(old.smooth, new.smooth);
            for (p, q) in [(old.anchor, new.anchor), (old.in_ctrl, new.in_ctrl), (old.out_ctrl, new.out_ctrl)] {
                assert!((q.x - (140.0 - p.y)).abs() < 1e-9);
                assert!((q.y - (p.x - 40.0)).abs() < 1e-9);
            }
        }
    }
}
#[test]
fn component_rotation_ui_tests_stale_rotated_scope_cancels_without_mutation() {
    for name in ["work", "One"] {
        for reason in ["path", "clear", "revision", "document", "layer", "tool", "reselect"] {
            let mut a = fixture(name);
            begin(&mut a, &egui::Context::default()).unwrap();
            rotate_drag(&mut a);
            match reason {
                "path" => a.ui.selected_path = Some("Two".into()),
                "clear" => a.ui.selected_path = None,
                "revision" => a.session.active_mut().unwrap().revision += 1,
                "document" => {
                    a.session.execute("file.new", json!({"width":100,"height":100})).unwrap();
                }
                "layer" => a.session.active_mut().unwrap().active_layer = None,
                "tool" => a.ui.tool = Tool::DirectSelection,
                _ => crate::work_path_drag::reset_scope(&mut a),
            }
            let docs = a.session.documents().iter().map(|s| (s.doc.clone(), s.history.past_len())).collect::<Vec<_>>();
            commit(&mut a);
            assert!(a.ui.transform.is_none());
            assert_eq!(a.session.documents().iter().map(|s| (s.doc.clone(), s.history.past_len())).collect::<Vec<_>>(), docs, "{reason}");
        }
    }
}
#[test]
fn component_rotation_ui_tests_presets_rotate_handles_and_unsupported_modes_stay_blocked() {
    for name in ["work", "One"] {
        for id in ["edit.transform.rotate", "edit.transform.rotate90Cw", "edit.transform.rotate90Ccw", "edit.transform.rotate180"] {
            let mut a = fixture(name);
            let ctx = egui::Context::default();
            begin(&mut a, &ctx).unwrap();
            assert!(crate::menus::is_enabled(&a, id));
            crate::menus::invoke(&mut a, &ctx, id, json!({})).unwrap();
            if id == "edit.transform.rotate" {
                let q = a.ui.transform.as_ref().unwrap().quad;
                let p = q[2];
                let c = a.ui.transform.as_ref().unwrap().pivot;
                let end = [c[0] - (p[1] - c[1]), c[1] + (p[0] - c[0])];
                pointer(&mut a, ToolEvent::Down { x: p[0], y: p[1], pressure: 1.0 }, egui::Modifiers::NONE);
                pointer(&mut a, ToolEvent::Up { x: end[0], y: end[1] }, egui::Modifiers::NONE);
            }
            assert!(component_matrix(a.ui.transform.as_ref().unwrap()).is_some());
            assert!(rotation_degrees(a.ui.transform.as_ref().unwrap().quad).abs() > 1.0);
            for bad in [
                "edit.transform.skew",
                "edit.transform.distort",
                "edit.transform.perspective",
                "edit.transform.warp",
                "edit.transform.flipHorizontal",
                "edit.transform.flipVertical",
            ] {
                let t = a.ui.transform.clone();
                assert!(!crate::menus::is_enabled(&a, bad));
                assert!(crate::menus::invoke(&mut a, &ctx, bad, json!({})).is_err());
                assert_eq!(a.ui.transform, t);
            }
        }
    }
}
#[test]
fn component_rotation_ui_tests_reflection_shear_and_small_perspective_never_preview_or_commit() {
    for kind in ["reflection", "shear", "perspective"] {
        let mut a = fixture("One");
        let before = path(&a, "One");
        let history = a.session.active().unwrap().history.past_len();
        begin(&mut a, &egui::Context::default()).unwrap();
        let t = a.ui.transform.as_mut().unwrap();
        match kind {
            "reflection" => t.quad = t.quad.map(|q| [280.0 - q[0], q[1]]),
            "shear" => {
                t.quad[0][0] += 10.0;
                t.quad[1][0] += 10.0;
            }
            _ => t.quad[2][0] += 0.001,
        }
        assert!(component_matrix(a.ui.transform.as_ref().unwrap()).is_none());
        commit(&mut a);
        assert!(a.ui.transform.is_some());
        assert_eq!(path(&a, "One"), before);
        assert_eq!(a.session.active().unwrap().history.past_len(), history);
        cancel(&mut a);
    }
}
#[test]
fn component_rotation_ui_tests_keyboard_cancel_and_commit_preserve_one_document_step() {
    let a = fixture("One");
    let original = path(&a, "One");
    let history = a.session.active().unwrap().history.past_len();
    let mut h = egui_kittest::Harness::builder().with_size(vec2(800.0, 600.0)).build_ui_state(
        |ui, a: &mut PhotocraftApp| {
            crate::shortcuts::handle(a, ui.ctx());
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label("Canvas focus");
            });
        },
        a,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(2);
    fn key(h: &mut egui_kittest::Harness<PhotocraftApp>, key: egui::Key, mods: egui::Modifiers) {
        h.event(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: mods });
        h.run_steps(1);
        h.event(egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: mods });
        h.run_steps(1);
    }
    key(&mut h, egui::Key::T, egui::Modifiers::COMMAND);
    rotate_drag(h.state_mut());
    key(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.state().ui.transform.is_none());
    assert_eq!(path(h.state(), "One"), original);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    key(&mut h, egui::Key::T, egui::Modifiers::COMMAND);
    rotate_drag(h.state_mut());
    key(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(h.state().ui.transform.is_none());
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    key(&mut h, egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(path(h.state(), "One"), original);
    key(&mut h, egui::Key::Z, egui::Modifiers { command: true, shift: true, ..Default::default() });
    assert_ne!(path(h.state(), "One"), original);
}
#[test]
fn component_rotation_ui_tests_scene_references_and_siblings_stay_fixed_with_optional_metal_render() {
    for all in [false, true] {
        let mut a = fixture("One");
        a.ui.show_all_paths = all;
        begin(&mut a, &egui::Context::default()).unwrap();
        let xf = ViewXform {
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0)),
            zoom: 1.0,
            center: [160.0, 150.0],
            flip: false,
            rotation: 0.0,
            aspect: 1.0,
        };
        let shapes = |a: &PhotocraftApp| {
            let ctx = egui::Context::default();
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let p = ui.ctx().layer_painter(egui::LayerId::background());
                crate::vector_ui::draw_overlay(a, &p, &xf, &a.session.active().unwrap().doc);
                draw_overlay(a, &p, &xf);
            });
            out.textures_delta.clear();
            out.shapes.into_iter().filter_map(|s| if let egui::Shape::Path(p) = s.shape { Some(p.points) } else { None }).collect::<Vec<_>>()
        };
        let before = shapes(&a);
        rotate_drag(&mut a);
        let during = shapes(&a);
        assert_eq!(before.len(), if all { 10 } else { 4 });
        assert_eq!(before.len(), during.len());
        assert_eq!(&before[..before.len() - 2], &during[..during.len() - 2]);
        assert_ne!(before[before.len() - 2], during[during.len() - 2]);
        if all && let Some(dir) = std::env::var_os("PHOTOCRAFT_COMPONENT_ROTATION_SNAPSHOT_DIR") {
            let mut h = egui_kittest::Harness::builder().with_size(vec2(1000.0, 600.0)).build_ui_state(
                |ui, a: &mut PhotocraftApp| {
                    egui::Panel::left("paths").exact_size(220.0).show(ui, |ui| crate::vector_ui::paths_panel(a, ui));
                    egui::Panel::top("options").exact_size(35.0).show(ui, |ui| options_bar(a, ui));
                    egui::CentralPanel::default().show(ui, |ui| {
                        let xf = ViewXform { rect: ui.max_rect(), zoom: 1.0, center: [160.0, 150.0], flip: false, rotation: 0.0, aspect: 1.0 };
                        crate::vector_ui::draw_overlay(a, ui.painter(), &xf, &a.session.active().unwrap().doc);
                        draw_overlay(a, ui.painter(), &xf);
                    });
                },
                a,
            );
            PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
            h.run_steps(3);
            h.render().unwrap().save(std::path::PathBuf::from(dir).join("component-rotation-preview.png")).unwrap();
        }
    }
}

#[test]
fn component_rotation_ui_tests_fractional_rotation_and_scaling_in_either_order_remain_supported() {
    for name in ["work", "One"] {
        for angle in [-123.4_f64, -37.5, 15.0, 37.5, 135.0, 180.0] {
            for first in [false, true] {
                let mut a = fixture(name);
                begin(&mut a, &egui::Context::default()).unwrap();
                if first {
                    scale_about_pivot(&mut a, 1.3, 0.7);
                }
                rotate_about_pivot(&mut a, angle.to_radians());
                assert!((rotation_degrees(a.ui.transform.as_ref().unwrap().quad) - angle).abs() < 1e-7);
                if !first {
                    scale_about_pivot(&mut a, 1.3, 0.7);
                }
                let t = a.ui.transform.as_ref().unwrap();
                let m = component_matrix(t).unwrap();
                assert!((m.m[0].hypot(m.m[1]) - 1.3).abs() < 1e-8);
                assert!((m.m[2].hypot(m.m[3]) - 0.7).abs() < 1e-8);
                let q = t.quad;
                let opposite = [q[0][0] - (q[2][0] - q[0][0]), q[0][1] - (q[2][1] - q[0][1])];
                pointer(&mut a, ToolEvent::Down { x: q[2][0], y: q[2][1], pressure: 1.0 }, egui::Modifiers::SHIFT);
                pointer(&mut a, ToolEvent::Up { x: opposite[0], y: opposite[1] }, egui::Modifiers::SHIFT);
                assert_eq!(a.ui.transform.as_ref().unwrap().quad, q, "scaling handle must not reverse both axes");
                commit(&mut a);
                assert!(a.ui.transform.is_none(), "{}", a.ui.status);
            }
        }
    }
}
#[test]
fn component_rotation_ui_tests_existing_angle_field_is_editable_without_skew_fields() {
    use egui_kittest::kittest::Queryable;
    for name in ["work", "One"] {
        let mut a = fixture(name);
        let original = path(&a, name);
        begin(&mut a, &egui::Context::default()).unwrap();
        scale_about_pivot(&mut a, 1.5, 0.75);
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1000.0, 360.0)).build_ui_state(
            |ui, a: &mut PhotocraftApp| {
                ui.horizontal_centered(|ui| options_bar(a, ui));
            },
            a,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(4);
        assert_eq!(h.get_all_by_role(egui::accesskit::Role::SpinButton).count(), 5, "X/Y/W/H/angle only");
        h.get_all_by_role(egui::accesskit::Role::SpinButton).nth(4).unwrap().click();
        h.run();
        for c in "37.5".chars() {
            h.event(egui::Event::Text(c.to_string()));
            h.run();
        }
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        let t = h.state().ui.transform.as_ref().unwrap();
        assert!((rotation_degrees(t.quad) - 37.5).abs() < 1e-6);
        let m = component_matrix(t).unwrap();
        assert!((m.m[0].hypot(m.m[1]) - 1.5).abs() < 1e-7);
        assert!((m.m[2].hypot(m.m[3]) - 0.75).abs() < 1e-7);
        assert_eq!(path(h.state(), name), original);
        commit(h.state_mut());
        assert!(h.state().ui.transform.is_none(), "{}", h.state().ui.status);
    }
}
