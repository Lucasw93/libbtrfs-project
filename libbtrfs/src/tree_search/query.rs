//! The Query trait exists so that both the `TreeSearch` struct and the
//! `BoxedTreeSearch` struct end up calling the same method to perform a tree search.
//! It is possible for the `SearchBuilder` to return a opaque type. However the returned
//! type would not be `dyn compatable` which would an issue in many cases.
//!
//! TODO: Both TreeSearch and BoxedTreeSearch are simmialr enough that this function should be
//! implemented here as a provided function. That would be safer. Right now the implementations
//! of the `query` methods are almost identical.
use super::*;

/// Implementors can perform tree searches via the [`query()`], method.
///
/// [`query()`]: Query::query
pub trait Query
{
    /// Returns an iterator yeilding instances of [`SearchItem`].
    ///
    /// The [`ExactSizeIterator::len()`] method can be used to check how many items the search
    /// returned.
    fn query<'buf, F, K>(
        &'buf mut self,
        on_drop: F,
    ) -> io::Result<impl Iterator<Item = SearchItem<'buf>> + ExactSizeIterator>
    where
        K: FromQueryKey,
        F: FnOnce(QueryKey) -> K;
}

/// Primary sort key for a btrfs item. Field 0 of [`QueryKey`] tuple.
pub type ObjectId = u64;
/// Secondary sort key for a btrfs item. Field 1 of [`QueryKey`] tuple.
pub type Ty = u32;
/// Tertiary sort key for a btrfs item. Field 2 of [`QueryKey`] tuple.
pub type Offset = u64;

/// Represents the sorting order, in wich items are returned from the [`Query::query()`] method.
pub type QueryKey = (ObjectId, Ty, Offset);

/// Set the search key from the last seen [`QueryKey`]
///
/// Return value for the closure called when the [`TreeSearch::query()`] iterator goes out of
/// scope. Sets the search key used for future calls to [`TreeSearch::query()`] based on the last
/// seen [`QueryKey`].
pub trait FromQueryKey
{
    #[doc(hidden)]
    fn as_query_key(self) -> Option<QueryKey>;
}

/// Does not update the search key.
impl FromQueryKey for Option<()>
{
    fn as_query_key(self) -> Option<QueryKey>
    {
        None
    }
}

/// Does not update the search key.
impl FromQueryKey for ()
{
    fn as_query_key(self) -> Option<QueryKey>
    {
        None
    }
}

/// Behavior not yet decided.
impl FromQueryKey for u64
{
    fn as_query_key(self) -> Option<QueryKey>
    {
        todo!()
    }
}

/// Updates the [`QueryKey`] with provided value.
impl FromQueryKey for QueryKey
{
    fn as_query_key(self) -> Option<QueryKey>
    {
        Some(self)
    }
}

/// Increment the key to the next objectid
///
/// Returns a [`QueryKey`] with the `objectid` incremeted by one from `key`. If the objectid for `key`
/// is [`u64::MAX`] then `key` is returned unmodified.
///
/// This is a utility function for the `on_drop` argument of the [`Query::query()`] method.
pub fn next_objectid(key: QueryKey) -> QueryKey
{
    u64::checked_add(key.0, 1).map_or(key, |obj| (obj, 0, 0))
}

/// Increment the key to the next type
///
/// Returns a [`QueryKey`] with the `type` incremeted by one from `key`. If the type for `key` is
/// [`u8::MAX`] then the `type` is set to 0 and the `objectid` is incremeted.
///
/// This is a utility function for the `on_drop` argument of the [`Query::query()`] method.
pub fn next_type(key: QueryKey) -> QueryKey
{
    u8::checked_add(key.1 as u8, 1).map_or_else(|| next_type(key), |ty| (key.0, ty as u32, 0))
}

/// Increment the key to the next offset
///
/// Returns a [`QueryKey`] with the `offset` incremeted by one from `key`. If the offset for `key`
/// is [`u64::MAX`] then the `offset` is set to 0 and the `type` is incremeted.
///
/// This is a utility function for the `on_drop` argument of the [`Query::query()`] method.
pub fn next_offset(key: QueryKey) -> QueryKey
{
    u64::checked_add(key.2, 1).map_or_else(|| next_type(key), |off| (key.0, key.1, off))
}
