use html5ever::driver::ParseOpts;
use html5ever::parse_document;
use html5ever::rcdom::{Handle, Node, NodeData, RcDom};
use html5ever::serialize;
use html5ever::serialize::SerializeOpts;
use html5ever::tendril::TendrilSink;
use layout::DeviceContext;
use std::{
    cell::RefCell,
    fmt::{Display, Formatter},
    fs::File,
};
use std::{fs, ops::Deref};
use std::{io::Write, rc::Rc};

async fn fetch() -> Result<(), Box<dyn std::error::Error>> {
    let url = "https://ja.wikipedia.org/wiki/%E3%83%95%E3%83%A9%E3%83%B3%E3%82%B7%E3%82%B9%E3%83%BB%E3%83%89%E3%83%AC%E3%83%BC%E3%82%AF#%E4%B8%96%E7%95%8C%E4%B8%80%E5%91%A8%E3%81%AE%E5%81%89%E6%A5%AD";
    let client = reqwest::Client::new();
    let resp = client.get(url).send().await?;

    let body = resp.text().await?;

    //let body = body.chars().collect::<Vec<char>>();
    //println!("{:?}", &body[..1000].iter().collect::<String>());

    let mut file = File::create("francis_wiki.html")?;
    file.write_all(body.as_bytes())?;

    Ok(())
}

fn load() -> String {
    let mut s = fs::read_to_string("francis_wiki.html").unwrap();

    s = s.replace("\n", "");
    s = s.replace("\t", "");

    s
}

fn pull_out(node: &Handle, elem_name: &str) -> bool {
    let indices = node
        .children
        .borrow()
        .iter()
        .enumerate()
        .filter(|&(_, child)| {
            if let NodeData::Element { ref name, .. } = child.data {
                if name.local.to_string() == elem_name.to_string() {
                    return true;
                }
            }
            return false;
        })
        .map(|(i, _)| i)
        .rev()
        .collect::<Vec<usize>>();

    for i in &indices {
        let children = node.children.borrow()[*i].children.borrow().clone();
        for (j, child) in children.iter().enumerate() {
            child.parent.set(Some(std::rc::Rc::downgrade(node)));
            node.children.borrow_mut().insert(i + j + 1, child.clone());
        }
        node.children.borrow_mut().remove(*i);
    }

    !indices.is_empty()
}

fn concatenate_text(node: &Handle) {
    let mut i = 1usize;
    let mut children = node.children.borrow().clone();

    while i < children.len() {
        if let NodeData::Text { ref contents } = children[i - 1].data {
            let contents1 = contents;
            if let NodeData::Text { ref contents } = children[i].data {
                contents1
                    .borrow_mut()
                    .push_tendril(&contents.borrow().deref());
                children.remove(i);

                continue;
            }
        }

        i += 1;
    }

    node.children.borrow_mut().clear();
    node.children.borrow_mut().extend(children);
}

fn trim_text(s: &str) -> String {
    let mut s = s.to_string();

    loop {
        let s1 = s.replace("  ", " ");
        if s1 == s {
            break;
        }
        s = s1;
    }

    s = s.trim().to_string();

    s
}

fn remove_decoration(node: &Handle) {
    match node.data {
        NodeData::Text { ref contents } => {
            let s = trim_text(&contents.borrow().to_string());

            if s.len() > 0 {
                //println!("{:?}", s);
            }
        }
        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } => {
            if name.local.to_string() == "a".to_string() {
                /*println!("{:?}", name.local.to_string());

                for attr in attrs.borrow().iter() {
                    println!(
                        "    {:?}: {:?}",
                        attr.name.local.to_string(),
                        attr.value.to_string()
                    );
                }

                if (*node.children.borrow()).len() > 0 {
                    let child = &(*node.children.borrow())[0];
                    if let NodeData::Text { ref contents } = child.data {
                        println!("{}", contents.borrow().to_string());
                    }
                }*/
            }
        }
        NodeData::Document { .. } => {}
        NodeData::Doctype { .. } => {}
        NodeData::Comment { .. } => {}
        NodeData::ProcessingInstruction { .. } => {}
    };

    let elem_names = vec!["a", "b", "i", "sup", "cite", "span"];
    while elem_names.iter().any(|elem_name| pull_out(node, elem_name)) {}

    concatenate_text(node);

    for child in node.children.borrow().iter() {
        remove_decoration(&child);
    }
}

fn find_elements(node: &Handle, elem_name: &str) -> Vec<Handle> {
    let mut vec: Vec<Handle> = vec![];

    match node.data {
        NodeData::Element { ref name, .. } => {
            if name.local.to_string() == elem_name.to_string() {
                vec.push(node.clone());
            }
        }
        _ => {}
    };

    for child in node.children.borrow().iter() {
        vec.extend(find_elements(&child, elem_name));
    }

    vec
}

fn collect_text(node: &Handle) -> String {
    let mut text = String::new();

    match node.data {
        NodeData::Text { ref contents } => {
            text = trim_text(&contents.borrow().to_string());
        }
        _ => {}
    };

    for child in node.children.borrow().iter() {
        text.push_str(&collect_text(&child));
    }

    text
}

fn get_elem_name(node: &Handle) -> String {
    match node.data {
        NodeData::Element { ref name, .. } => {
            return name.local.to_string();
        }
        _ => {}
    };

    String::new()
}

fn get_attr(node: &Handle, attr_name: &str) -> Option<String> {
    match node.data {
        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } => {
            for attr in attrs.borrow().iter() {
                if attr.name.local.to_string() == attr_name.to_string() {
                    return Some(attr.value.to_string());
                }
            }
        }
        _ => {}
    };

    None
}

mod layout {
    use std::{
        cell::{Cell, RefCell},
        rc::{Rc, Weak},
    };

    #[derive(Debug)]
    pub struct Size {
        pub width: u32,
        pub height: u32,
    }

    impl Size {
        pub fn new() -> Self {
            Size {
                width: 0,
                height: 0,
            }
        }
    }

    #[derive(Debug)]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }

    impl Point {
        pub fn new() -> Self {
            Point { x: 0, y: 0 }
        }
    }

    pub struct Region {
        pos: Point,
        size: Size,
    }

    impl Region {
        pub fn new() -> Self {
            Region {
                pos: Point::new(),
                size: Size::new(),
            }
        }
    }

    pub struct Resizable {
        pub size: Size,
        pub min_size: Size,
        pub max_size: Size,
        pub expand_h: bool,
        pub expand_v: bool,
    }

    impl Resizable {
        pub fn new() -> Self {
            Resizable {
                size: Size::new(),
                min_size: Size {
                    width: 0,
                    height: 0,
                },
                max_size: Size {
                    width: 0,
                    height: 0,
                },
                expand_h: false,
                expand_v: false,
            }
        }
    }
    pub enum Orient {
        H,
        V,
    }

    pub enum BlockData {
        Space,
        Sizer { orient: Orient },
        Text { text: String },
    }

    type Handle = Rc<Block>;
    type WeakHandle = Weak<Block>;

    pub struct Block {
        pub parent: Cell<Option<WeakHandle>>,
        pub children: RefCell<Vec<Handle>>,
        pub data: BlockData,
        pub pos: Point,
        pub size: Resizable,
    }

    impl Block {
        pub fn new_from(data: BlockData) -> Self {
            Block {
                parent: Cell::new(None),
                children: RefCell::new(vec![]),
                data: data,
                pos: Point::new(),
                size: Resizable::new(),
            }
        }
    }

    pub trait DeviceContext {
        fn measure_text(&self, text: &str) -> Size;
    }

    pub struct TestDC {}

    impl TestDC {
        pub fn new() -> Self {
            TestDC {}
        }
    }

    impl DeviceContext for TestDC {
        fn measure_text(&self, text: &str) -> Size {
            let lines: Vec<&str> = text.split('\n').collect();
            let max_len = lines
                .iter()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0);

            Size {
                width: 20 * max_len as u32,
                height: 20 * lines.len() as u32,
            }
        }
    }
}

#[derive(Debug)]
struct TextBlock {
    text: String,
    pos: layout::Point,
    size: layout::Size,
    min_width: u32,
    max_width: u32,
}

impl TextBlock {
    fn new_from(text: &str) -> TextBlock {
        let dc = layout::TestDC::new();
        let size = dc.measure_text(text);
        let min_width = dc.measure_text(" ").width;
        let max_width = size.width;

        TextBlock {
            text: text.to_string(),
            pos: layout::Point::new(),
            size: size,
            min_width: min_width,
            max_width: max_width,
        }
    }
}

struct TableCell {
    text_block: TextBlock,
    row_range: Vec<u32>,
    col_range: Vec<u32>,
}

impl TableCell {
    fn new_from(text_block: TextBlock) -> Self {
        TableCell {
            text_block: text_block,
            row_range: vec![],
            col_range: vec![],
        }
    }
}

impl Display for TableCell {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(
            f,
            "Cell = row_range: {:?}, col_range: {:?}\n {:?}",
            self.row_range, self.col_range, self.text_block
        )
    }
}

struct Table {
    rows: u32,
    cols: u32,
    min_width_cols: Vec<u32>,
    max_width_cols: Vec<u32>,
    cells: Vec<TableCell>,
}

impl Table {
    fn new() -> Self {
        Table {
            rows: 0,
            cols: 0,
            min_width_cols: vec![],
            max_width_cols: vec![],
            cells: vec![],
        }
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        writeln!(
            f,
            "Table = rows: {}, cols: {}, min_width_cols: {:?}, max_width_cols: {:?}",
            self.rows, self.cols, self.min_width_cols, self.max_width_cols
        )?;

        for (i, cell) in self.cells.iter().enumerate() {
            writeln!(f, "{}: {:}\n", i, cell)?;
        }

        Ok(())
    }
}

#[test]
fn test() {
    let dc = layout::TestDC::new();
    let size = dc.measure_text("ab\nc");
    println!("{:?}", size);
}

#[tokio::main]
async fn main() -> () {
    //fetch().await.expect("");
    let html_data = load();
    let parser = parse_document(RcDom::default(), ParseOpts::default());
    let dom = parser.one(html_data);

    //println!("{}", dom.document.children.borrow().len());

    let node = &dom.document.children.borrow()[1];
    remove_decoration(node);

    let table_nodes = find_elements(node, "table");
    for table_node in table_nodes {
        let tbody_node = find_elements(&table_node, "tbody");
        if tbody_node.len() != 1 {
            continue;
        }

        let tbody_node = tbody_node[0].clone();
        let tr_nodes = find_elements(&tbody_node, "tr");

        let mut table = Table::new();

        table.rows = tr_nodes.len() as u32;

        for (row, tr_node) in tr_nodes.iter().enumerate() {
            let mut col = 0u32;
            for child in tr_node.children.borrow().iter() {
                let tag_name = get_elem_name(child);
                if tag_name != "th" && tag_name != "td" {
                    continue;
                }

                let colspan = match get_attr(child, "colspan") {
                    Some(s) => s.parse::<u32>().unwrap_or(1),
                    _ => 1,
                };

                let col_range = col..(col + colspan);
                col += colspan;

                let text = collect_text(&child);
                if text.is_empty() {
                    continue;
                }

                let mut cell = TableCell::new_from(TextBlock::new_from(&text));
                cell.row_range.push(row as u32);
                cell.col_range.extend(col_range);

                table.cells.push(cell);
            }
        }

        if let Some(v) = table
            .cells
            .iter()
            .map(|cell| cell.col_range.iter().max().unwrap_or(&0))
            .max()
        {
            table.cols = v + 1;
        };

        println!("{:}", table);
        println!("------------------------------");
    }

    //let mut bytes = vec![];
    //serialize(&mut bytes, &dom.document, SerializeOpts::default()).unwrap();
    //println!("{}", String::from_utf8(bytes).unwrap());
}
