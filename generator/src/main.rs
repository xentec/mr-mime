//! Generates the `segments.rs` file for interned strings.

use fastrand::Rng;
use heck::{AsShoutySnakeCase, AsSnakeCase, AsUpperCamelCase, ToUpperCamelCase};
use intern_str::builder::{Builder, IgnoreCase, Utf8Graph};
use memchr::memchr;

use std::collections::{
    hash_map::{Entry, HashMap},
    HashSet,
};
use std::env;
use std::fmt;
use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter};

fn main() -> io::Result<()> {
    // Ensure that the process is deterministic using a set key.
    let rng = Rng::with_seed(0xD3ADB33F);

    // Determine the files to read from/write to.
    let mut args = env::args_os().skip(1);
    let input = args.next().unwrap_or_else(|| "mime.types".into());
    let output = args.next().unwrap_or_else(|| "segments.rs".into());

    // Open the input file.
    let input = File::open(input)?;
    let input = BufReader::new(input);

    // Read MIME types from the file.
    let mime_types = input
        .lines()
        .filter_map(|line| {
            line.map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    None
                } else {
                    let mut parts = line.split_whitespace();
                    let ty = parts.next().unwrap().to_string();

                    Mime::parse(ty, parts.map(|s| s.to_string()).collect())
                }
            })
            .transpose()
        })
        .collect::<io::Result<Vec<_>>>()?;

    // Open the output file.
    let output = File::create(output)?;
    let mut output = BufWriter::new(output);

    // Begin writing to the output.
    writeln!(
        output,
        "// This file is automatically generated by `mr-mime-generator`. Do not edit.\n"
    )?;

    // Write enums for the MIME types.
    write_mime_part(
        &mut output,
        "TypeIntern",
        &mime_types,
        |ty| Some(&ty.ty),
        true,
        &rng,
    )?;
    write_mime_part(
        &mut output,
        "SubtypeIntern",
        &mime_types,
        |ty| Some(&ty.subtype),
        true,
        &rng,
    )?;
    write_mime_part(
        &mut output,
        "SuffixIntern",
        &mime_types,
        |ty| ty.suffix.as_deref(),
        false,
        &rng,
    )?;

    // Write `MIME` type constants.
    writeln!(output)?;
    writeln!(output, "/// Constants for common MIME types and subtypes.")?;
    writeln!(output, "pub mod constants {{")?;

    let mut existing_names = HashSet::new();
    let mut existing_types = HashSet::new();

    // Write the primary types.

    writeln!(output, "{}/// Common MIME type prefixes.", Indent(1))?;
    writeln!(output, "{}pub mod types {{", Indent(1))?;

    for mime in &mime_types {
        if !existing_types.insert(mime.ty.clone()) {
            continue;
        }

        writeln!(output, "{}/// The `{}` MIME type.", Indent(2), mime.ty)?;
        writeln!(
            output,
            "{}pub const {}: crate::Type<'static> = crate::Type(crate::Name::Interned(crate::TypeIntern::{}));",
            Indent(2),
            AsShoutySnakeCase(&mime.ty),
            AsUpperCamelCase(&mime.ty),
        )?;
        writeln!(output)?;
    }

    writeln!(output, "{}}}", Indent(1))?;
    writeln!(output)?;

    // Write the subtypes.
    existing_types.clear();
    writeln!(output, "{}/// Common MIME subtypes.", Indent(1))?;
    writeln!(output, "{}pub mod subtypes {{", Indent(1))?;

    for mime in &mime_types {
        if mime
            .subtype
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
            .is_none()
        {
            continue;
        }

        if !existing_types.insert(mime.subtype.to_ascii_lowercase()) {
            continue;
        }

        writeln!(
            output,
            "{}/// The `{}` MIME subtype.",
            Indent(2),
            mime.subtype
        )?;
        writeln!(
            output,
            "{}pub const {}: crate::Subtype<'static> = crate::Subtype(crate::Name::Interned(crate::SubtypeIntern::{}));",
            Indent(2),
            AsShoutySnakeCase(&mime.subtype),
            AsUpperCamelCase(&mime.subtype),
        )?;
        writeln!(output)?;
    }

    writeln!(output, "{}}}", Indent(1))?;
    writeln!(output)?;

    // Write the suffixes.
    existing_types.clear();
    writeln!(output, "{}/// Common MIME suffixes.", Indent(1))?;
    writeln!(output, "{}pub mod suffixes {{", Indent(1))?;

    for mime in &mime_types {
        if let Some(suffix) = &mime.suffix {
            if !existing_types.insert(suffix.to_ascii_lowercase()) {
                continue;
            }

            match mime
                .suffix
                .as_ref()
                .map(|s| s.to_upper_camel_case().to_lowercase())
                .as_deref()
            {
                Some("hdr") | Some("src") => continue,
                _ => {}
            }

            writeln!(output, "{}/// The `{}` MIME suffix.", Indent(2), suffix)?;
            writeln!(
                output,
                "{}pub const {}: crate::Suffix<'static> = crate::Suffix(crate::Name::Interned(crate::SuffixIntern::{}));",
                Indent(2),
                AsShoutySnakeCase(suffix),
                AsUpperCamelCase(suffix),
            )?;
            writeln!(output)?;
        }
    }

    writeln!(output, "{}}}", Indent(1))?;
    writeln!(output)?;

    for mime in &mime_types {
        if mime
            .subtype
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
            .is_none()
        {
            continue;
        }

        match mime
            .suffix
            .as_ref()
            .map(|s| s.to_upper_camel_case().to_lowercase())
            .as_deref()
        {
            Some("hdr") | Some("src") => continue,
            _ => {}
        }

        let name = mime.name();

        if !existing_names.insert(name.clone()) {
            continue;
        }

        writeln!(output, "{}/// `{}`", Indent(1), mime,)?;
        writeln!(
            output,
            "{}pub const {}: crate::Mime<'static> = crate::Mime(crate::Repr::Parts {{",
            Indent(1),
            name,
        )?;
        writeln!(
            output,
            "{}ty: crate::Name::Interned(super::TypeIntern::{}),",
            Indent(2),
            AsUpperCamelCase(&mime.ty),
        )?;
        writeln!(
            output,
            "{}subtype: crate::Name::Interned(super::SubtypeIntern::{}),",
            Indent(2),
            AsUpperCamelCase(&mime.subtype),
        )?;
        writeln!(
            output,
            "{}suffix: {},",
            Indent(2),
            match mime.suffix {
                Some(ref suffix) => format!(
                    "Some(crate::Name::Interned(super::SuffixIntern::{}))",
                    AsUpperCamelCase(suffix)
                ),
                None => "None".to_string(),
            },
        )?;
        writeln!(output, "{}parameters: &[]", Indent(2))?;
        writeln!(output, "{}}});", Indent(1))?;
        writeln!(output)?;

        writeln!(output, "{}#[test]", Indent(1))?;
        writeln!(output, "{}fn {}_parse() {{", Indent(1), AsSnakeCase(&name))?;

        // Parse the MIME type as a string.
        let mime_txt = mime.to_string();
        writeln!(
            output,
            "{}assert_eq!(crate::Mime::parse(\"{}\"), Ok({}));",
            Indent(2),
            &mime_txt,
            name,
        )?;

        let mime_text = random_case_str(&mime_txt, &rng);
        writeln!(
            output,
            "{}assert_eq!(crate::Mime::parse(\"{}\"), Ok({}));",
            Indent(2),
            mime_text,
            name,
        )?;

        writeln!(output, "{}}}", Indent(1))?;
        writeln!(output)?;
    }

    writeln!(output, "}}")?;

    // Write the "guess" method.
    guess_function(&mut output, &mime_types)?;
    writeln!(output)?;

    Ok(())
}

fn write_mime_part(
    output: &mut impl Write,
    name: &str,
    types: &[Mime],
    get_field: impl Fn(&Mime) -> Option<&str>,
    has_star: bool,
    rng: &Rng,
) -> io::Result<()> {
    // Get an iterator over every possible value.
    let mut types = types
        .iter()
        .filter_map(get_field)
        .filter(|name| {
            name.chars()
                .next()
                .filter(|c| c.is_ascii_alphabetic())
                .is_some()
        })
        .map(|name| (name, name.to_upper_camel_case()))
        .collect::<Vec<_>>();
    types.sort_unstable_by(|a, b| a.1.cmp(&b.1));
    types.dedup_by(|a, b| a.1 == b.1);

    // Write out the enum.
    writeln!(
        output,
        "#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]"
    )?;
    writeln!(output, "pub(crate) enum {} {{", name)?;

    // Write asterisk.
    if has_star {
        writeln!(output, "{}Star,", Indent(1))?;
    }

    // Write out each member.
    for (_, field) in &types {
        writeln!(output, "{}{},", Indent(1), field)?;
    }

    writeln!(output, "}}")?;

    // Begin implementation work.
    writeln!(output)?;
    writeln!(output, "impl {} {{", name)?;

    // Write out an "as_str" method.
    writeln!(
        output,
        "{}pub(crate) fn as_str(self) -> &'static str {{",
        Indent(1)
    )?;
    writeln!(output, "{}match self {{", Indent(2))?;

    if has_star {
        writeln!(output, "{}{}::Star => \"*\",", Indent(3), name)?;
    }

    for (realtext, field) in &types {
        writeln!(
            output,
            "{}{}::{} => \"{}\",",
            Indent(3),
            name,
            field,
            realtext
        )?;
    }

    writeln!(output, "{}}}", Indent(2))?;
    writeln!(output, "{}}}", Indent(1))?;
    writeln!(output, "}}")?;

    // Write out a "from_str" method.
    writeln!(output, "impl core::str::FromStr for {} {{", name)?;
    writeln!(output, "{}type Err = crate::InvalidName;", Indent(1))?;
    writeln!(output)?;
    writeln!(
        output,
        "{}fn from_str(s: &str) -> Result<Self, Self::Err> {{",
        Indent(1)
    )?;

    // Begin creating the graph.
    let mut builder = Builder::<_, IgnoreCase<Utf8Graph>>::new();

    if has_star {
        builder.add("*".to_string(), "Star").ok();
    }

    for (realtext, field) in &types {
        builder.add(realtext.to_string(), field).ok();
    }

    let mut buffer = vec![];
    let graph = builder.build(&mut buffer);

    // Write out the graph.
    let outname = format!("Option<{}>", name);
    let generated = intern_str_codegen::generate(
        &graph,
        "intern_str::CaseInsensitive<&'static str>",
        &outname,
        |f, n| match n.as_ref() {
            None => write!(f, "None"),
            Some(n) => write!(f, "Some({}::{})", name, n),
        },
    );
    writeln!(
        output,
        "{}const GRAPH: intern_str::Graph<'static, 'static, intern_str::CaseInsensitive<&'static str>, {}> = {};",
        Indent(2),
        &outname,
        generated
    )?;

    // Write out the lookup.
    writeln!(
        output,
        "{}GRAPH.process(intern_str::CaseInsensitive(s)).as_ref().copied().ok_or(crate::InvalidName)",
        Indent(2)
    )?;
    writeln!(output, "{}}}", Indent(1))?;

    writeln!(output, "}}")?;
    writeln!(output)?;

    // Add a test for the string parser.
    writeln!(output, "#[test]")?;
    writeln!(output, "fn {}_from_str() {{", AsSnakeCase(name))?;

    if has_star {
        writeln!(
            output,
            "{}assert_eq!(\"*\".parse::<{}>(), Ok({}::Star));",
            Indent(1),
            name,
            name
        )?;
    }

    for (realtext, field) in &types {
        writeln!(
            output,
            "{}assert_eq!(\"{}\".parse::<{}>(), Ok({}::{}));",
            Indent(1),
            realtext,
            name,
            name,
            field,
        )?;

        // We should also parse with random spacing.
        let field_next = random_case_str(realtext, rng);

        writeln!(
            output,
            "{}assert_eq!(\"{}\".parse::<{}>(), Ok({}::{}));",
            Indent(1),
            field_next,
            name,
            name,
            field,
        )?;
    }

    writeln!(output, "}}")?;
    writeln!(output)?;

    // Add an AsRef<str> impl.
    writeln!(output, "impl AsRef<str> for {} {{", name)?;
    writeln!(
        output,
        "{}fn as_ref(&self) -> &str {{ self.as_str() }}",
        Indent(1)
    )?;
    writeln!(output, "}}")?;
    writeln!(output)?;

    // Add an Into<&'static str> impl
    writeln!(output, "impl From<{}> for &'static str {{", name)?;
    writeln!(
        output,
        "{}fn from(name: {}) -> Self {{ name.as_str() }}",
        Indent(1),
        name
    )?;
    writeln!(output, "}}")?;
    writeln!(output)?;

    Ok(())
}

/// Write the "guess" function for MIME types.
fn guess_function(out: &mut impl Write, mimes: &[Mime]) -> io::Result<()> {
    // We want a map between the extension and the MIME type, so reverse the slice.
    let mut map: HashMap<_, Vec<&Mime>> = HashMap::new();

    for mime in mimes {
        if mime
            .subtype
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
            .is_none()
        {
            continue;
        }

        match mime
            .suffix
            .as_ref()
            .map(|s| s.to_upper_camel_case().to_lowercase())
            .as_deref()
        {
            Some("hdr") | Some("src") => continue,
            _ => {}
        }

        for ext in &mime.extensions {
            match map.entry(ext) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().push(mime);
                }
                Entry::Vacant(entry) => {
                    entry.insert(vec![mime]);
                }
            }
        }
    }

    // Create a case-insensitive string intern map.
    let mut builder = Builder::<_, IgnoreCase<Utf8Graph>>::new();

    for (ext, entries) in map.iter() {
        builder.add(ext.to_string(), entries).ok();
    }

    let mut buffer = vec![];
    let graph = builder.build(&mut buffer);

    // Begin writing the function.
    writeln!(
        out,
        "pub(super) fn guess_mime_type(ext: &str) -> Option<&'static [crate::Mime<'static>]> {{"
    )?;

    // Write out the graph.
    let input_name = "intern_str::CaseInsensitive<&'static str>";
    let output_name = "Option<&'static [crate::Mime<'static>]>";

    let generated = intern_str_codegen::generate(&graph, input_name, output_name, |f, n| match n {
        None => write!(f, "None"),
        Some(n) => {
            write!(f, "Some(&[")?;

            for (i, mime) in n.iter().enumerate() {
                if i != 0 {
                    write!(f, ", ")?;
                }

                write!(f, "constants::{}", mime.name())?;
            }

            write!(f, "])")
        }
    });

    writeln!(
        out,
        "{}const GRAPH: intern_str::Graph<'static, 'static, {}, {}> = {};",
        Indent(1),
        input_name,
        output_name,
        generated
    )?;

    writeln!(
        out,
        "{}GRAPH.process(intern_str::CaseInsensitive(ext)).as_ref().copied()",
        Indent(1)
    )?;

    writeln!(out, "}}")?;

    Ok(())
}

struct Mime {
    /// The MIME type.
    ty: String,

    /// The MIME subtype.
    subtype: String,

    /// The MIME suffix.
    suffix: Option<String>,

    /// The MIME extensions.
    extensions: Vec<String>,
}

impl Mime {
    /// Parses a MIME type from a string.
    fn parse(mut s: String, extensions: Vec<String>) -> Option<Self> {
        // Split the MIME type off.
        let slash = memchr(b'/', s.as_bytes())?;
        let rest = s.split_off(slash + 1);
        let mut ty = s;
        ty.pop();
        s = rest;

        // Now all we have to do it split the subtype and suffix by +
        let (subtype, suffix) = if let Some(plus) = memchr(b'+', s.as_bytes()) {
            if plus == s.len() - 1 {
                (s, None)
            } else {
                let rest = s.split_off(plus + 1);
                let mut subtype = s;
                subtype.pop();
                (subtype, Some(rest))
            }
        } else {
            (s, None)
        };

        Some(Self {
            ty,
            subtype,
            suffix,
            extensions,
        })
    }

    fn name(&self) -> String {
        format!(
            "{}_{}{}",
            AsShoutySnakeCase(&self.ty),
            AsShoutySnakeCase(&self.subtype),
            match self.suffix {
                Some(ref suffix) => format!("_{}", AsShoutySnakeCase(suffix)),
                None => String::new(),
            },
        )
    }
}

impl fmt::Display for Mime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.ty, self.subtype)?;

        if let Some(suffix) = &self.suffix {
            write!(f, "+{}", suffix)?;
        }
        Ok(())
    }
}

struct Indent(usize);

impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for _ in 0..self.0 {
            write!(f, "    ")?;
        }
        Ok(())
    }
}

/// Randomize the case of a string.
fn random_case_str(a: &str, rng: &Rng) -> String {
    a.chars()
        .map(|c| {
            if rng.bool() {
                c.to_ascii_lowercase()
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect()
}
