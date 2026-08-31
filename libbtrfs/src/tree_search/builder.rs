use super::*;

/// private module to get a `btrfs_ioctl_search_key`
pub mod seal
{
    pub trait SearchKeyBuilderExt
    {
        fn get_key(&mut self) -> &mut super::btrfs_ioctl_search_key;
    }
}

macro_rules! tree_search_set_key_by_range {
    ($arg:ident;  $__self:ident . $get_key_fn:ident -> $min:ident | $max:ident) => {{
        match $arg.start_bound() {
            Bound::Unbounded => {
                if let Bound::Included(&b) = $arg.end_bound() {
                    $__self.$get_key_fn().$min = b;
                    $__self.$get_key_fn().$max = b;

                    return $__self;
                }
            }
            Bound::Excluded(&b) | Bound::Included(&b) => {
                $__self.$get_key_fn().$min = b;
            }
        }
        if let Bound::Included(&b) | Bound::Excluded(&b) = $arg.end_bound() {
            $__self.$get_key_fn().$max = b;
        };

        $__self
    }};
}

/// This trait provides methods used to set and update the search key used for a Btrfs Tree Search.
///
/// Search key fields that are used to set minimum and maximum bounds for a Tree Search can be set
/// using the Rust [`std::ops::RangeBounds`] syntax, where the `start_bound` will set the lower
/// bounds and `end_bound` will set the higher bound. All ranges are treated as inclusive, and
/// unbounded ranges are ignored. The special `..=` syntax can be used to set both the minimum
/// and maximum bounds to the same things.
///
/// # Example
///
/// The following example shows how the minimum and maximum bounds can be set for the `objectid`
/// key field.
///
/// ```no_run,rustfmt::skip
/// use libbtrfs::tree_search::{SearchBuilder, SearchKeyBuilder, TreeId};
///
/// SearchBuilder::from_path("/")?
///     // search the root tree
///     .tree(TreeId::RootTree)
///
///     // return at most 20 items
///     .item_limit(20)
///
///     // set the minimum objectid to 256 and the maximum objectid to u64::MAX
///     .objectid(256..u64::MAX)
///
///     // as above
///     .objectid(256..=u64::MAX)
///
///     // sets the minimum objectid to 500 and the maximum objectid is left unchanged
///     .objectid(500..)
///
///     // sets the maximum objectid to 2000 and the minimum is left unchanged
///     .objectid(..2000)
///
///     // sets BOTH minimum and maximum objectid to 1000
///     .objectid(..=1000)
///
///     // consume the builder and return a TreeSearch
///     .build();
///
/// Ok::<(), std::io::Error>(())
/// ````
pub trait SearchKeyBuilder: seal::SearchKeyBuilderExt + Sized
{
    /// Set the tree to be searched.
    ///
    /// Default is `TreeId::RootTree`
    fn tree(mut self, tree_id: TreeId) -> Self
    {
        self.get_key().tree_id = tree_id as u64;
        self
    }

    /// Limit the number of items that the search will find.
    ///
    /// Default is `u32::MAX`
    fn item_limit(mut self, limit: u32) -> Self
    {
        self.get_key().nr_items = limit;
        self
    }

    /// Set the minimum and maximum offset bounds.
    ///
    /// Default is `u64::MIN..u64::MAX`
    fn offset(mut self, offset: impl RangeBounds<u64>) -> Self
    {
        tree_search_set_key_by_range!(offset; self.get_key -> min_offset | max_offset)
    }

    /// Set the minimum and maximum objectid bounds.
    ///
    /// Default is `u64::MIN..u64::MAX`
    fn objectid(mut self, objectid: impl RangeBounds<u64>) -> Self
    {
        tree_search_set_key_by_range!(objectid; self.get_key -> min_objectid | max_objectid)
    }

    /// Set the minimum and maximum transid bounds.
    ///
    /// Default is `u64::MIN..u64::MAX`
    fn transid(mut self, transid: impl RangeBounds<u64>) -> Self
    {
        tree_search_set_key_by_range!(transid; self.get_key -> min_transid | max_transid)
    }

    /// Set the minimum and maximum type bounds.
    ///
    /// Default is `u32::MIN..u32::MAX`
    fn item_type(mut self, item_type: impl RangeBounds<u32>) -> Self
    {
        tree_search_set_key_by_range!(item_type; self.get_key -> min_type | max_type)
    }
}

/// Constructs either a [`TreeSearch`] or a [`BoxedTreeSearch`]
pub struct SearchBuilder<R: AsFd>
{
    key: btrfs_ioctl_search_key,
    resource: R,
}

impl<R: AsFd> seal::SearchKeyBuilderExt for SearchBuilder<R>
{
    #[inline(always)]
    fn get_key(&mut self) -> &mut self::btrfs_ioctl_search_key
    {
        &mut self.key
    }
}

impl<R: AsFd> SearchKeyBuilder for SearchBuilder<R> {}

impl SearchBuilder<File>
{
    /// Constructs a new `SearchBuilder` from a path.
    ///
    /// This is fallible, see [`SearchBuilder::new()`] for an infallible variant.
    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<Self>
    {
        Ok(Self {
            key: Default::default(),
            resource: File::open(path)?,
        })
    }
}

impl<R: AsFd> SearchBuilder<R>
{
    /// Constructs a new `SearchBuilder` from an IO resouce
    pub fn new(resource: R) -> Self
    {
        Self { key: Default::default(), resource }
    }

    /// Build a new [`TreeSearch`].
    pub fn build(self) -> TreeSearch<R>
    {
        let mut args = MaybeUninit::<btrfs_ioctl_search_args>::uninit();
        let argp = args.as_mut_ptr();
        unsafe {
            write(&raw mut (*argp).key, self.key);
        }
        TreeSearch { args, resource: self.resource }
    }

    /// Build a new [`BoxedTreeSearch`]. `BoxedTreeSearch` contains heap allocated memory which is
    /// baed on `buf_size`.
    pub fn build_boxed(self, buf_size: u64) -> BoxedTreeSearch<R>
    {
        let size = buf_size as usize + size_of::<btrfs_ioctl_search_args_v2>();
        let align = align_of::<btrfs_ioctl_search_args_v2>();

        let layout = Layout::from_size_align(size, align).unwrap();

        let args = unsafe {
            let args = alloc(layout).cast::<btrfs_ioctl_search_args_v2>();

            if args.is_null() {
                handle_alloc_error(layout)
            }
            write(&raw mut (*args).key, self.key);
            write(&raw mut (*args).buf_size, buf_size);

            args
        };

        BoxedTreeSearch { args, resource: self.resource }
    }
}
