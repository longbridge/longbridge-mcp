//! Small BM25-style index shared by `search` (tool catalogue) and `docs`
//! (documentation snapshot). ASCII is split on non-alphanumerics; CJK runs
//! emit unigrams and bigrams so Chinese queries match without a dictionary.

use std::collections::HashMap;

#[allow(dead_code)]
const K1: f32 = 1.2;
#[allow(dead_code)]
const B: f32 = 0.75;

/// One weighted text field of a document.
#[allow(dead_code)]
pub(crate) struct Field {
    pub weight: f32,
    pub text: String,
}

/// A document to index, identified by `key`.
#[allow(dead_code)]
pub(crate) struct Doc {
    pub key: String,
    pub fields: Vec<Field>,
}

/// A ranked search hit.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub(crate) struct Hit {
    pub key: String,
    pub score: f32,
}

#[allow(dead_code)]
struct Posting {
    doc: usize,
    weighted_tf: f32,
}

/// Inverted index over weighted fields.
#[allow(dead_code)]
pub(crate) struct Index {
    keys: Vec<String>,
    lengths: Vec<f32>,
    avg_length: f32,
    postings: HashMap<String, Vec<Posting>>,
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F)
}

/// Lower-cased tokens: ASCII words plus CJK unigrams and bigrams.
#[allow(dead_code)]
pub(crate) fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut ascii = String::new();
    let mut cjk: Vec<char> = Vec::new();
    let flush_ascii = |ascii: &mut String, out: &mut Vec<String>| {
        if !ascii.is_empty() {
            out.push(std::mem::take(ascii));
        }
    };
    let flush_cjk = |cjk: &mut Vec<char>, out: &mut Vec<String>| {
        for c in cjk.iter() {
            out.push(c.to_string());
        }
        for pair in cjk.windows(2) {
            out.push(pair.iter().collect());
        }
        cjk.clear();
    };
    for c in text.chars() {
        if is_cjk(c) {
            flush_ascii(&mut ascii, &mut out);
            cjk.push(c);
        } else if c.is_alphanumeric() {
            flush_cjk(&mut cjk, &mut out);
            ascii.extend(c.to_lowercase());
        } else {
            flush_ascii(&mut ascii, &mut out);
            flush_cjk(&mut cjk, &mut out);
        }
    }
    flush_ascii(&mut ascii, &mut out);
    flush_cjk(&mut cjk, &mut out);
    out
}

impl Index {
    /// Build the index; `docs` order defines internal ids.
    #[allow(dead_code)]
    pub(crate) fn build(docs: Vec<Doc>) -> Self {
        let mut keys = Vec::with_capacity(docs.len());
        let mut lengths = Vec::with_capacity(docs.len());
        let mut postings: HashMap<String, Vec<Posting>> = HashMap::new();
        for (doc_id, doc) in docs.into_iter().enumerate() {
            keys.push(doc.key);
            let mut tf: HashMap<String, f32> = HashMap::new();
            let mut length = 0.0;
            for field in doc.fields {
                for token in tokenize(&field.text) {
                    *tf.entry(token).or_insert(0.0) += field.weight;
                    length += field.weight;
                }
            }
            lengths.push(length);
            for (token, weighted_tf) in tf {
                postings.entry(token).or_default().push(Posting {
                    doc: doc_id,
                    weighted_tf,
                });
            }
        }
        let avg_length = if lengths.is_empty() {
            1.0
        } else {
            lengths.iter().sum::<f32>() / lengths.len() as f32
        };
        Self {
            keys,
            lengths,
            avg_length,
            postings,
        }
    }

    /// Number of indexed documents.
    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }

    /// Top `limit` documents by BM25 score, descending; empty for an empty
    /// query or no matching term.
    #[allow(dead_code)]
    pub(crate) fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let n = self.keys.len() as f32;
        let mut scores: HashMap<usize, f32> = HashMap::new();
        for token in tokenize(query) {
            let Some(list) = self.postings.get(&token) else {
                continue;
            };
            let df = list.len() as f32;
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            for posting in list {
                let norm = K1 * (1.0 - B + B * self.lengths[posting.doc] / self.avg_length);
                let tf = posting.weighted_tf;
                *scores.entry(posting.doc).or_insert(0.0) += idf * tf * (K1 + 1.0) / (tf + norm);
            }
        }
        let mut hits: Vec<Hit> = scores
            .into_iter()
            .map(|(doc, score)| Hit {
                key: self.keys[doc].clone(),
                score,
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.key.cmp(&b.key))
        });
        hits.truncate(limit);
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(key: &str, name: &str, desc: &str) -> Doc {
        Doc {
            key: key.into(),
            fields: vec![
                Field {
                    weight: 10.0,
                    text: name.into(),
                },
                Field {
                    weight: 3.0,
                    text: desc.into(),
                },
            ],
        }
    }

    #[test]
    fn tokenize_splits_ascii_and_emits_cjk_unigrams_and_bigrams() {
        assert_eq!(
            tokenize("Quote_Snapshot v2"),
            vec!["quote", "snapshot", "v2"]
        );
        assert_eq!(tokenize("行情"), vec!["行", "情", "行情"]);
        assert_eq!(
            tokenize("市场温度 index"),
            vec!["市", "场", "温", "度", "市场", "场温", "温度", "index"]
        );
    }

    #[test]
    fn name_match_outranks_description_match() {
        let index = Index::build(vec![
            doc("quote", "quote", "Get latest price quotes"),
            doc("depth", "depth", "Order book depth; quote levels"),
        ]);
        let hits = index.search("quote", 10);
        assert_eq!(hits[0].key, "quote");
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn chinese_query_matches_chinese_text() {
        let index = Index::build(vec![
            doc("quote", "quote", "行情快照，返回最新价"),
            doc("market_temperature", "market_temperature", "市场温度"),
        ]);
        let hits = index.search("市场温度", 10);
        assert_eq!(hits[0].key, "market_temperature");
        let hits = index.search("行情", 10);
        assert_eq!(hits[0].key, "quote");
    }

    #[test]
    fn empty_query_or_no_match_returns_empty() {
        let index = Index::build(vec![doc("quote", "quote", "x")]);
        assert!(
            index.search("", 10).is_empty(),
            "empty query should return no hits"
        );
        assert!(
            index.search("zzzz", 10).is_empty(),
            "unmatched query should return no hits"
        );
    }

    #[test]
    fn limit_is_respected() {
        let docs = (0..20)
            .map(|i| doc(&format!("t{i}"), "quote", "q"))
            .collect();
        let index = Index::build(docs);
        assert_eq!(index.search("quote", 5).len(), 5);
    }
}
