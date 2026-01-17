pub(crate) fn extract_column_names<I, T, F>(columns: I, name: F) -> Vec<String>
where
    I: IntoIterator<Item = T>,
    F: Fn(&T) -> &str,
{
    columns
        .into_iter()
        .map(|col| name(&col).to_string())
        .collect()
}
