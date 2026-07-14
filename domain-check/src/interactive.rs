use crate::Args;
use domain_check_lib::normalize_tld;
use std::io::{BufRead, Write};

pub fn run_wizard(args: &mut Args) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stderr = std::io::stderr();
    let mut output = stderr.lock();
    run_wizard_with_io(args, &mut input, &mut output)
}

fn run_wizard_with_io(
    args: &mut Args,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<(), String> {
    writeln!(output, "Domain candidate wizard").map_err(io_error)?;
    let (min_length, max_length) =
        prompt_until(input, output, "Kaç harf? (5 veya 5-7): ", |value| {
            parse_length(value).ok_or("5-10 arasında tek sayı veya min-max aralığı girin")
        })?;
    if min_length == max_length {
        args.length = Some(min_length);
    } else {
        args.min_length = Some(min_length);
        args.max_length = Some(max_length);
    }

    let required = prompt_line(input, output, "Zorunlu harfler (boş bırakılabilir): ")?
        .trim()
        .to_ascii_lowercase();
    if !required.is_empty() {
        validate_filter_text(&required)?;
        let placement = prompt_until(
            input,
            output,
            "Konum [1=herhangi, 2=başta, 3=sonda]: ",
            |value| match value.trim().to_ascii_lowercase().as_str() {
                "1" | "herhangi" | "any" => Ok(1),
                "2" | "başta" | "basta" | "start" => Ok(2),
                "3" | "sonda" | "end" => Ok(3),
                _ => Err("1, 2 veya 3 girin"),
            },
        )?;
        match placement {
            1 => args.contains = Some(required),
            2 => args.starts_with = Some(required),
            3 => args.ends_with = Some(required),
            _ => unreachable!(),
        }
    }

    let generate_count = prompt_positive(input, output, "Kaç aday üretilsin?: ")?;
    let top = prompt_until(
        input,
        output,
        "Kaç en iyi aday işlensin/sorgulansın?: ",
        |value| {
            let parsed = value
                .trim()
                .parse::<usize>()
                .map_err(|_| "pozitif bir sayı girin")?;
            if parsed == 0 || parsed > generate_count {
                Err("değer 1 ile üretilen aday sayısı arasında olmalı")
            } else {
                Ok(parsed)
            }
        },
    )?;
    let tld = prompt_until(input, output, "TLD [com]: ", |value| {
        let value = if value.trim().is_empty() {
            "com"
        } else {
            value
        };
        normalize_tld(value).ok_or("geçerli tek bir TLD girin")
    })?;
    let score_only = prompt_until(
        input,
        output,
        "Mod [1=score-only, 2=gerçek RDAP/WHOIS sorgusu]: ",
        |value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "score" | "score-only" => Ok(true),
            "2" | "query" | "sorgu" | "real" => Ok(false),
            _ => Err("1 veya 2 girin"),
        },
    )?;

    args.generate_count = Some(generate_count);
    args.top = Some(top);
    args.tlds = Some(vec![tld]);
    args.score_only = score_only;
    args.score = !score_only;
    args.interactive = false;
    Ok(())
}

fn parse_length(value: &str) -> Option<(usize, usize)> {
    let parts: Vec<_> = value.trim().split('-').collect();
    let parsed = match parts.as_slice() {
        [one] => {
            let value = one.trim().parse().ok()?;
            (value, value)
        }
        [min, max] => (min.trim().parse().ok()?, max.trim().parse().ok()?),
        _ => return None,
    };
    (parsed.0 >= 5 && parsed.1 <= 10 && parsed.0 <= parsed.1).then_some(parsed)
}

fn prompt_positive(
    input: &mut impl BufRead,
    output: &mut impl Write,
    prompt: &str,
) -> Result<usize, String> {
    prompt_until(input, output, prompt, |value| {
        let parsed = value
            .trim()
            .parse::<usize>()
            .map_err(|_| "pozitif bir sayı girin")?;
        (parsed > 0)
            .then_some(parsed)
            .ok_or("sayı sıfırdan büyük olmalı")
    })
}

fn prompt_until<T>(
    input: &mut impl BufRead,
    output: &mut impl Write,
    prompt: &str,
    parse: impl Fn(&str) -> Result<T, &str>,
) -> Result<T, String> {
    loop {
        let value = prompt_line(input, output, prompt)?;
        match parse(value.trim()) {
            Ok(parsed) => return Ok(parsed),
            Err(message) => writeln!(output, "Geçersiz giriş: {message}").map_err(io_error)?,
        }
    }
}

fn prompt_line(
    input: &mut impl BufRead,
    output: &mut impl Write,
    prompt: &str,
) -> Result<String, String> {
    write!(output, "{prompt}").map_err(io_error)?;
    output.flush().map_err(io_error)?;
    let mut value = String::new();
    if input.read_line(&mut value).map_err(io_error)? == 0 {
        return Err("giriş beklenirken stdin kapandı".to_string());
    }
    Ok(value)
}

fn validate_filter_text(value: &str) -> Result<(), String> {
    if value.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        Ok(())
    } else {
        Err("zorunlu harfler yalnızca a-z içerebilir".to_string())
    }
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn wizard_maps_answers_to_cli_arguments() {
        let mut args = crate::tests::create_test_args();
        args.interactive = true;
        let mut input = Cursor::new(b"5\nai\n1\n5000\n500\n\n2\n");
        let mut output = Vec::new();
        run_wizard_with_io(&mut args, &mut input, &mut output).unwrap();
        assert_eq!(args.length, Some(5));
        assert_eq!(args.contains.as_deref(), Some("ai"));
        assert_eq!(args.generate_count, Some(5000));
        assert_eq!(args.top, Some(500));
        assert_eq!(args.tlds, Some(vec!["com".to_string()]));
        assert!(!args.score_only);
        assert!(args.score);
    }
}
