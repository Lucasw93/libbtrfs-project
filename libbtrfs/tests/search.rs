use libbtrfs::tree_search::{SearchBuilder, SearchKeyBuilder, TreeId, tree_item::*};

fn _print_item_type(key: u32)
{
    eprintln!("{:=<70}", "");
    match key {
        DirItem::KEY => eprintln!(" BTRFS_DIR_ITEM_KEY ({})", key),
        DirIndex::KEY => eprintln!(" BTRFS_DIR_INDEX_KEY ({})", key),
        RootItem::KEY => eprintln!(" BTRFS_ROOT_ITEM_KEY ({})", key),
        RootRef::KEY => eprintln!(" BTRFS_ROOT_REF_KEY ({})", key),
        RootBackref::KEY => eprintln!(" BTRFS_ROOT_BACKREF_KEY ({})", key),
        _ => {
            eprintln!(" UNKNOWN: {} ", key)
        }
    }
}

#[test]
fn search_boxed() -> std::io::Result<()>
{
    let nalloc = 64 * 1024;
    let nr_items = 20;
    eprintln!("RUNNING BOXED SARCH FOR {nr_items} ITEMS WITH BUF SIZE: {nalloc}");

    SearchBuilder::try_from("/")?
        .tree(TreeId::RootTree)
        .item_limit(20)
        .objectid(256..)
        .new_boxed(nalloc)
        .search(|_| None)?
        .for_each(|item| {
            eprintln!("{:=<77}", "");
            eprintln!(
                "\n (objectid: {},  type: {},  offset: {}  --  transid: {},  len: {}) \n",
                item.objectid(),
                item.ty(),
                item.offset(),
                item.transid(),
                item.len(),
            );

            if let Some(ri) = item.get::<RootItem>() {
                eprintln!(" - UUID: {:?}", ri.uuid())
            } else if let Some(rr) = item.get::<RootRef>() {
                eprintln!(" - name: {:?}", rr.name_str())
            }
        });

    Ok(())
}

#[test]
fn search_subvols_example() -> std::io::Result<()>
{
    let mut search = SearchBuilder::try_from("/")?
        .item_limit(u32::MAX)
        .tree(TreeId::RootTree)
        .objectid(5..u64::MAX)
        .new();

    loop {
        let items = search.search(|(objectid, ..)| (objectid + 1, 0, 0))?;

        if items.len() == 0 {
            break;
        }

        for item in items {
            if let Some(rr) = item.get::<RootRef>() {
                println!(
                    "ID {} level {} name {}",
                    item.offset(),
                    item.objectid(),
                    rr.name_str()?
                )
            }
        }
    }

    Ok(())
}
