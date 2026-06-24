pub(crate) fn to_pascal_case(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

pub(crate) fn to_snake_case(name: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = true;

    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if character.is_ascii_uppercase() && !previous_was_separator {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            output.push('_');
            previous_was_separator = true;
        }
    }

    if output.ends_with('_') {
        output.pop();
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{to_pascal_case, to_snake_case};

    #[test]
    fn snake_case_normalizes_cli_artifact_names() {
        assert_eq!(to_snake_case("user-profile"), "user_profile");
        assert_eq!(to_snake_case("user.profile"), "user_profile");
        assert_eq!(to_snake_case("UserProfile"), "user_profile");
        assert_eq!(to_snake_case("!!!"), "");
    }

    #[test]
    fn pascal_case_derives_rust_type_names() {
        assert_eq!(to_pascal_case("user_profile"), "UserProfile");
        assert_eq!(to_pascal_case("user-2"), "User2");
    }
}
