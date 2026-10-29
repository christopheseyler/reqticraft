use rc_plugin::{discover, Output};

#[test]
fn converts_through_a_discovered_plugin() {
    let root = std::env::temp_dir().join(format!("rc-plugin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let plugin_dir = root.join("plugins").join("demo");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let exe = env!("CARGO_BIN_EXE_reqticraft-plugin-demo").replace('\\', "/");
    std::fs::write(
        plugin_dir.join("plugin.ini"),
        format!("[plugin]\nname=demo\ncommand=\"{exe}\"\ntools=Lines\n"),
    )
    .unwrap();
    std::fs::write(root.join("reqs.txt"), "R1 : Démarrer\nR2 : Arrêter\n").unwrap();

    let plugins = discover(&root);
    let demo = plugins
        .iter()
        .find(|p| p.provides("lines"))
        .expect("plugin trouvé");
    let out = demo
        .convert("Lines", "reqs.txt", &[], &root, &root.join("w"))
        .unwrap();
    assert_eq!(
        out,
        Output {
            xml: true,
            content: "<reqs><req id=\"R1\">Démarrer</req><req id=\"R2\">Arrêter</req></reqs>"
                .into()
        }
    );
    let err = demo
        .convert("Lines", "absent.txt", &[], &root, &root.join("w"))
        .unwrap_err();
    assert!(err.to_string().contains("absent.txt"));
}
