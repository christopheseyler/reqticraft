//! Commande externe : l'intermédiaire est produit par un outil tiers
//! (export d'un référentiel propriétaire, script maison…). C'est aussi le
//! moyen d'accès générique des types d'accès sans connecteur dédié
//! (ClearCase, Synergy, PVCS, Documentum, ProjectWise, DesignSync…).
//!
//! Le modèle de commande vient du paramètre `command` ou de l'option `command`
//! du profil. Marques remplacées : `{location}` (emplacement du document),
//! `{output}` (fichier à produire), `{workdir}`. Sans `{output}`, la sortie
//! standard est l'intermédiaire. La commande est lancée sans interpréteur de
//! commandes ; les guillemets regroupent les arguments.

use crate::{ConnectError, Context, Result};
use rc_convert::Intermediate;

pub fn run(location: &str, ctx: &Context) -> Result<Intermediate> {
    let template = match ctx.param("command") {
        Some(c) => c.to_owned(),
        None => ctx
            .profile(false)?
            .and_then(|p| p.option("command").map(str::to_owned))
            .ok_or_else(|| ConnectError::msg("paramètre « command » requis"))?,
    };
    std::fs::create_dir_all(&ctx.work_dir).map_err(|e| ConnectError::io(&ctx.work_dir, e))?;
    let output = ctx.work_dir.join("output");
    let text = execute(&template, location, ctx, &output)?;
    Ok(match ctx.param("format") {
        Some(f) if f.eq_ignore_ascii_case("xml") => Intermediate::Xml(text),
        Some(_) => Intermediate::Text(text),
        None if text.trim_start().starts_with('<') => Intermediate::Xml(text),
        None => Intermediate::Text(text),
    })
}

/// Exécute le modèle et renvoie le texte produit.
pub fn execute(
    template: &str,
    location: &str,
    ctx: &Context,
    output: &std::path::Path,
) -> Result<String> {
    let args: Vec<String> = split(template)
        .into_iter()
        .map(|a| {
            a.replace("{location}", location)
                .replace("{output}", &output.display().to_string())
                .replace("{workdir}", &ctx.work_dir.display().to_string())
        })
        .collect();
    let (program, rest) = args
        .split_first()
        .ok_or_else(|| ConnectError::msg("commande vide"))?;
    let refs: Vec<&str> = rest.iter().map(String::as_str).collect();
    let stdout = crate::run_tool(program, &refs, Some(ctx.base_dir), &[])?;
    if template.contains("{output}") {
        let bytes = std::fs::read(output).map_err(|e| ConnectError::io(output, e))?;
        return Ok(rc_interop::text::decode(&bytes));
    }
    Ok(rc_interop::text::decode(&stdout))
}

/// Découpe une ligne de commande : espaces, guillemets simples ou doubles.
pub fn split(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut any = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                any = true;
            }
            (None, c) if c.is_whitespace() => {
                if any || !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        args.push(cur);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Profiles;

    #[test]
    fn splits_command_lines() {
        assert_eq!(
            split(r#"tool -x "a b" 'c d' {output}"#),
            vec!["tool", "-x", "a b", "c d", "{output}"]
        );
        assert_eq!(split(r#"x """#), vec!["x", ""]);
    }

    #[test]
    fn runs_a_command_and_reads_its_output() {
        let dir = std::env::temp_dir().join(format!("rc-cmd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let profiles = Profiles::default();
        let params = vec![("command".to_owned(), "git --version".to_owned())];
        let ctx = Context {
            base_dir: &dir,
            work_dir: dir.join("w"),
            profiles: &profiles,
            params: &params,
        };
        match run("x", &ctx).unwrap() {
            Intermediate::Text(t) => assert!(t.starts_with("git version")),
            other => panic!("{other:?}"),
        }
    }
}
