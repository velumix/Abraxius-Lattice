//! Incremental full-text index. Deterministic lookups precede this layer in retrieval.

use std::path::Path;

use lattice_resource::{ContentHash, ResourceRef};
use serde::{Deserialize, Serialize};
use tantivy::{
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term,
    directory::MmapDirectory,
    doc,
    query::QueryParser,
    schema::{Field, STORED, STRING, Schema, TEXT, Value},
};
use thiserror::Error;

const WRITER_MEMORY_BYTES: usize = 20_000_000;

#[derive(Clone, Debug)]
struct Fields {
    resource_ref: Field,
    path: Field,
    name: Field,
    source: Field,
    symbols: Field,
    content_hash: Field,
}

pub struct SourceIndex {
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    fields: Fields,
}

#[derive(Clone, Debug)]
pub struct IndexDocument<'a> {
    pub resource_ref: &'a ResourceRef,
    pub path: &'a str,
    pub name: &'a str,
    pub source: &'a str,
    pub symbols: &'a str,
    pub content_hash: ContentHash,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextSearchHit {
    pub resource_ref: ResourceRef,
    pub path: String,
    pub name: String,
    pub content_hash: ContentHash,
    pub score_milli: i64,
}

impl SourceIndex {
    /// Opens or creates the source index at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError`] for directory, schema, reader, or writer failures.
    pub fn open(path: &Path) -> Result<Self, SearchError> {
        std::fs::create_dir_all(path)?;
        let (schema, fields) = create_schema();
        let directory = MmapDirectory::open(path)?;
        let index = Index::open_or_create(directory, schema)?;
        let reader =
            index.reader_builder().reload_policy(ReloadPolicy::OnCommitWithDelay).try_into()?;
        let writer = index.writer(WRITER_MEMORY_BYTES)?;
        Ok(Self { index, reader, writer, fields })
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.reader.searcher().num_docs() == 0
    }

    /// Atomically replaces the indexed document for one canonical resource.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError`] if the document cannot be written or committed.
    pub fn upsert(&mut self, document: &IndexDocument<'_>) -> Result<(), SearchError> {
        self.writer.delete_term(Term::from_field_text(
            self.fields.resource_ref,
            &document.resource_ref.to_string(),
        ));
        self.writer.add_document(doc!(
            self.fields.resource_ref => document.resource_ref.to_string(),
            self.fields.path => document.path,
            self.fields.name => document.name,
            self.fields.source => document.source,
            self.fields.symbols => document.symbols,
            self.fields.content_hash => document.content_hash.to_string(),
        ))?;
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Searches bounded indexed fields and returns compact stored metadata.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError`] for invalid query syntax, search failures, or a
    /// corrupt stored document.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<TextSearchHit>, SearchError> {
        let limit = limit.clamp(1, 100);
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(
            &self.index,
            vec![self.fields.name, self.fields.path, self.fields.symbols, self.fields.source],
        );
        let parsed = parser.parse_query(query)?;
        let top = searcher
            .search(&parsed, &tantivy::collector::TopDocs::with_limit(limit).order_by_score())?;
        top.into_iter()
            .map(|(score, address)| {
                let document: TantivyDocument = searcher.doc(address)?;
                let resource_ref =
                    stored_text(&document, self.fields.resource_ref, "resource_ref")?.parse()?;
                let hash = ContentHash::from_hex(stored_text(
                    &document,
                    self.fields.content_hash,
                    "content_hash",
                )?)?;
                Ok(TextSearchHit {
                    resource_ref,
                    path: stored_text(&document, self.fields.path, "path")?.to_owned(),
                    name: stored_text(&document, self.fields.name, "name")?.to_owned(),
                    content_hash: hash,
                    score_milli: score_milli(score),
                })
            })
            .collect()
    }
}

#[allow(clippy::cast_possible_truncation)]
fn score_milli(score: f32) -> i64 {
    // Tantivy scores are finite f32 values; an i64 easily contains every f32
    // integer after the deliberately low 1000x display scaling used here.
    (f64::from(score) * 1000.0).round() as i64
}

fn create_schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let resource_ref = builder.add_text_field("resource_ref", STRING | STORED);
    let path = builder.add_text_field("path", TEXT | STORED);
    let name = builder.add_text_field("name", TEXT | STORED);
    let source = builder.add_text_field("source", TEXT);
    let symbols = builder.add_text_field("symbols", TEXT);
    let content_hash = builder.add_text_field("content_hash", STRING | STORED);
    (builder.build(), Fields { resource_ref, path, name, source, symbols, content_hash })
}

fn stored_text<'a>(
    document: &'a TantivyDocument,
    field: Field,
    label: &'static str,
) -> Result<&'a str, SearchError> {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .ok_or(SearchError::MissingStoredField(label))
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("search directory error: {0}")]
    Directory(#[from] tantivy::directory::error::OpenDirectoryError),
    #[error("search index error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("search query error: {0}")]
    Query(#[from] tantivy::query::QueryParserError),
    #[error("stored search document is missing {0}")]
    MissingStoredField(&'static str),
    #[error("stored resource reference is invalid: {0}")]
    Resource(#[from] lattice_resource::ResourceRefError),
}

#[derive(Clone, Debug)]
struct ToolFields {
    tool_ref: Field,
    provider_id: Field,
    native_name: Field,
    title: Field,
    description: Field,
    capabilities: Field,
    schema_properties: Field,
    trust: Field,
    availability: Field,
}

pub struct ToolIndex {
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    fields: ToolFields,
}

#[derive(Clone, Debug)]
pub struct ToolIndexDocument<'a> {
    pub tool_ref: &'a str,
    pub provider_id: &'a str,
    pub native_name: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub capabilities: &'a str,
    pub schema_properties: &'a str,
    pub trust: &'a str,
    pub availability: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolTextSearchHit {
    pub tool_ref: String,
    pub provider_id: String,
    pub native_name: String,
    pub title: String,
    pub trust: String,
    pub availability: String,
    pub score_milli: i64,
}

impl ToolIndex {
    pub fn open(path: &Path) -> Result<Self, SearchError> {
        std::fs::create_dir_all(path)?;
        let (schema, fields) = create_tool_schema();
        let directory = MmapDirectory::open(path)?;
        let index = Index::open_or_create(directory, schema)?;
        let reader =
            index.reader_builder().reload_policy(ReloadPolicy::OnCommitWithDelay).try_into()?;
        let writer = index.writer(WRITER_MEMORY_BYTES)?;
        Ok(Self { index, reader, writer, fields })
    }

    pub fn upsert(&mut self, document: &ToolIndexDocument<'_>) -> Result<(), SearchError> {
        self.stage_tool(document)?;
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn upsert_batch(&mut self, documents: &[ToolIndexDocument<'_>]) -> Result<(), SearchError> {
        for document in documents {
            self.stage_tool(document)?;
        }
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    fn stage_tool(&mut self, document: &ToolIndexDocument<'_>) -> Result<(), SearchError> {
        self.writer.delete_term(Term::from_field_text(self.fields.tool_ref, document.tool_ref));
        self.writer.add_document(doc!(
            self.fields.tool_ref => document.tool_ref,
            self.fields.provider_id => document.provider_id,
            self.fields.native_name => document.native_name,
            self.fields.title => document.title,
            self.fields.description => document.description,
            self.fields.capabilities => document.capabilities,
            self.fields.schema_properties => document.schema_properties,
            self.fields.trust => document.trust,
            self.fields.availability => document.availability,
        ))?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<ToolTextSearchHit>, SearchError> {
        let parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.native_name,
                self.fields.title,
                self.fields.description,
                self.fields.capabilities,
                self.fields.schema_properties,
            ],
        );
        let parsed = parser.parse_query(query)?;
        let searcher = self.reader.searcher();
        let top = searcher.search(
            &parsed,
            &tantivy::collector::TopDocs::with_limit(limit.clamp(1, 100)).order_by_score(),
        )?;
        top.into_iter()
            .map(|(score, address)| {
                let document: TantivyDocument = searcher.doc(address)?;
                Ok(ToolTextSearchHit {
                    tool_ref: stored_text(&document, self.fields.tool_ref, "tool_ref")?.into(),
                    provider_id: stored_text(&document, self.fields.provider_id, "provider_id")?
                        .into(),
                    native_name: stored_text(&document, self.fields.native_name, "native_name")?
                        .into(),
                    title: stored_text(&document, self.fields.title, "title")?.into(),
                    trust: stored_text(&document, self.fields.trust, "trust")?.into(),
                    availability: stored_text(&document, self.fields.availability, "availability")?
                        .into(),
                    score_milli: score_milli(score),
                })
            })
            .collect()
    }
}

fn create_tool_schema() -> (Schema, ToolFields) {
    let mut builder = Schema::builder();
    let fields = ToolFields {
        tool_ref: builder.add_text_field("tool_ref", STRING | STORED),
        provider_id: builder.add_text_field("provider_id", STRING | STORED),
        native_name: builder.add_text_field("native_name", TEXT | STORED),
        title: builder.add_text_field("title", TEXT | STORED),
        description: builder.add_text_field("description", TEXT),
        capabilities: builder.add_text_field("capabilities", TEXT),
        schema_properties: builder.add_text_field("schema_properties", TEXT),
        trust: builder.add_text_field("trust", STRING | STORED),
        availability: builder.add_text_field("availability", STRING | STORED),
    };
    (builder.build(), fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_resource::{LatticeId, ResourceKind};

    #[test]
    fn an_upsert_replaces_the_previous_revision() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let mut index = SourceIndex::open(temporary.path())?;
        let reference =
            ResourceRef::workspace(LatticeId::new(), ResourceKind::Script, LatticeId::new());
        index.upsert(&IndexDocument {
            resource_ref: &reference,
            path: "Inventory.luau",
            name: "Inventory",
            source: "local old_inventory_marker = true",
            symbols: "old_inventory_marker",
            content_hash: ContentHash::of(b"old"),
        })?;
        index.upsert(&IndexDocument {
            resource_ref: &reference,
            path: "Inventory.luau",
            name: "Inventory",
            source: "local current_inventory_marker = true",
            symbols: "current_inventory_marker",
            content_hash: ContentHash::of(b"new"),
        })?;
        assert!(index.search("old_inventory_marker", 10)?.is_empty());
        assert_eq!(index.search("current_inventory_marker", 10)?.len(), 1);
        Ok(())
    }

    #[test]
    fn tool_catalog_search_scales_without_loading_schemas() -> Result<(), Box<dyn std::error::Error>>
    {
        let temporary = tempfile::tempdir()?;
        let mut index = ToolIndex::open(temporary.path())?;
        let fixtures = (0..1_000)
            .map(|number| {
                (
                    format!("lattice://provider/provider_test/tool/tool_{number:04}"),
                    format!("operation_{number:04}"),
                    if number == 742 {
                        "execute luau runtime".to_owned()
                    } else {
                        "fixture tool".to_owned()
                    },
                )
            })
            .collect::<Vec<_>>();
        let documents = fixtures
            .iter()
            .map(|(tool_ref, name, description)| ToolIndexDocument {
                tool_ref,
                provider_id: "provider_test",
                native_name: name,
                title: name,
                description,
                capabilities: "",
                schema_properties: "value",
                trust: "untrusted",
                availability: "available",
            })
            .collect::<Vec<_>>();
        index.upsert_batch(&documents)?;
        let matches = index.search("execute luau", 10)?;
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].native_name, "operation_0742");
        Ok(())
    }
}
