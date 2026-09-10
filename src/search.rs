use crate::config::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemRef {
    pub category: usize,
    pub item: usize,
}

pub fn results(config: &Config, query: &str) -> Vec<ItemRef> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for (category_index, category) in config.categories.iter().enumerate() {
        for (item_index, item) in category.items.iter().enumerate() {
            let name = item.name.to_lowercase();
            let path = item.path.to_lowercase();
            if let Some(score) = match_score(&name, &path, &query) {
                matches.push((
                    score,
                    ItemRef {
                        category: category_index,
                        item: item_index,
                    },
                ));
            }
        }
    }
    matches.sort_by_key(|entry| entry.0);
    matches.into_iter().map(|entry| entry.1).collect()
}

fn match_score(name: &str, path: &str, query: &str) -> Option<(u8, usize, usize)> {
    if name == query {
        return Some((0, 0, name.len()));
    }
    if let Some(position) = name.find(query) {
        return Some((1, position, name.len()));
    }
    if let Some(gaps) = subsequence_gaps(name, query) {
        return Some((2, gaps, name.len()));
    }
    if let Some(position) = path.find(query) {
        return Some((3, position, path.len()));
    }
    subsequence_gaps(path, query).map(|gaps| (4, gaps, path.len()))
}

fn subsequence_gaps(text: &str, query: &str) -> Option<usize> {
    let mut query_chars = query.chars();
    let mut expected = query_chars.next()?;
    let mut matched = 0usize;
    let mut gaps = 0usize;
    for character in text.chars() {
        if character == expected {
            matched += 1;
            if let Some(next) = query_chars.next() {
                expected = next;
            } else {
                return Some(gaps);
            }
        } else if matched > 0 {
            gaps += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LaunchItem;

    #[test]
    fn exact_name_wins_over_path_and_subsequence() {
        let mut config = Config::default();
        config.categories[0].items = vec![
            LaunchItem {
                name: "VS Code".into(),
                path: "C:\\Apps\\code.exe".into(),
                ..LaunchItem::default()
            },
            LaunchItem {
                name: "code".into(),
                path: "C:\\Other.exe".into(),
                ..LaunchItem::default()
            },
        ];
        let matches = results(&config, "code");
        assert_eq!(matches[0].item, 1);
    }

    #[test]
    fn sequence_match_works() {
        assert!(match_score("visual studio code", "", "vsc").is_some());
        assert!(match_score("visual studio code", "", "xyz").is_none());
    }
}
