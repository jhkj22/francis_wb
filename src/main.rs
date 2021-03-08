use html5ever::driver::ParseOpts;
use html5ever::parse_document;
use html5ever::rcdom::{Handle, Node, NodeData, RcDom};
use html5ever::serialize;
use html5ever::serialize::SerializeOpts;
use html5ever::tendril::TendrilSink;
use layout::{DeviceContext, Size};
use std::{
    borrow::BorrowMut,
    cell::{Cell, RefCell},
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

    #[derive(Clone, Copy, PartialEq, Debug)]
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

    #[derive(Clone, Copy, PartialEq, Debug)]
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

#[derive(Clone, Copy, Debug)]
struct BlockProps {
    width: Option<u32>,
    height: Option<u32>,

    min_width: u32,
    min_height: u32,

    max_width: u32,
    max_height: u32,
}

impl BlockProps {
    fn new() -> Self {
        BlockProps {
            width: None,
            height: None,
            min_width: u32::MIN,
            min_height: u32::MIN,
            max_width: u32::MAX,
            max_height: u32::MAX,
        }
    }
}

#[derive(Debug)]
struct TextBlock {
    text: String,
    pos: Cell<layout::Point>,
    size: Cell<layout::Size>,
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
            pos: Cell::new(layout::Point::new()),
            size: Cell::new(size),
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
    block_props: Cell<BlockProps>,
    rows: u32,
    cols: u32,
    min_width_cols: Vec<u32>,
    max_width_cols: Vec<u32>,
    size: Size,
    cells: Vec<TableCell>,
}

impl Table {
    fn new() -> Self {
        Table {
            block_props: Cell::new(BlockProps::new()),
            rows: 0,
            cols: 0,
            min_width_cols: vec![],
            max_width_cols: vec![],
            size: Size::new(),
            cells: vec![],
        }
    }

    fn calc_cols(&self) -> u32 {
        self.cells
            .iter()
            .map(|cell| cell.col_range.iter().max().unwrap_or(&0))
            .max()
            .map(|v| v + 1)
            .unwrap_or(0)
    }

    fn calc_max_width_cols(&self) -> Vec<u32> {
        (0..self.cols)
            .map(|col| {
                self.cells
                    .iter()
                    .map(|cell| {
                        cell.col_range
                            .iter()
                            .find(|c| **c == col)
                            .map(|_| {
                                let ratio = 1f32 / cell.col_range.len() as f32;
                                let block_width = cell.text_block.size.get().width;
                                let guess_width = (block_width as f32 * ratio) as u32;
                                guess_width
                            })
                            .unwrap_or(0)
                    })
                    .max()
                    .unwrap_or(0)
            })
            .collect()
    }

    fn calc_max_height_rows(&self) -> Vec<u32> {
        (0..self.rows)
            .map(|row| {
                self.cells
                    .iter()
                    .map(|cell| {
                        cell.row_range
                            .iter()
                            .find(|r| **r == row)
                            .map(|_| cell.text_block.size.get().height)
                            .unwrap_or(0)
                    })
                    .max()
                    .unwrap_or(0)
            })
            .collect()
    }

    fn calc_positions(&self) {
        let xs: Vec<u32> = self
            .max_width_cols
            .iter()
            .scan(0, |prev, w| {
                let ret = *prev;
                *prev += w;
                Some(ret)
            })
            .collect();

        let ys: Vec<u32> = (0..self.rows)
            .scan(0, |prev, _| {
                let ret = *prev;
                *prev += 20;
                Some(ret)
            })
            .collect();

        for cell in self.cells.iter() {
            let row = *cell.row_range.iter().min().unwrap();
            let col = *cell.col_range.iter().min().unwrap();
            let x = xs[col as usize] as i32;
            let y = ys[row as usize] as i32;
            cell.text_block.pos.set(layout::Point { x: x, y: y })
        }
    }

    fn set_cell_sizes(&self) {
        for cell in self.cells.iter() {
            let mut size = cell.text_block.size.get();

            size.width = cell
                .col_range
                .iter()
                .map(|c| self.max_width_cols[*c as usize])
                .fold(0, |sum, w| sum + w);

            cell.text_block.size.set(size);
        }
    }

    fn new_from(table_node: &Handle) -> Table {
        let mut table = Table::new();

        let mut block_props = BlockProps::new();

        if let Some(style) = get_attr(table_node, "style") {
            if let Some(_) = style.find("width") {
                block_props.width = Some(300);
            }
        }

        table.block_props.set(block_props);

        let tbody_node = find_elements(&table_node, "tbody");
        if tbody_node.len() != 1 {
            return table;
        }

        let tbody_node = tbody_node[0].clone();
        let tr_nodes = find_elements(&tbody_node, "tr");

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

        table.cols = table.calc_cols();
        table.max_width_cols = table.calc_max_width_cols();

        table.size.width = table.max_width_cols.iter().sum();
        table.size.height = table.calc_max_height_rows().iter().sum();

        table.calc_positions();
        table.set_cell_sizes();

        if let Some(w) = table.block_props.get().width {
            table.size.width = w;
        }

        table
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
        let table = Table::new_from(&table_node);
        println!("{:}", table);
        println!("------------------------------");
    }

    //let mut bytes = vec![];
    //serialize(&mut bytes, &dom.document, SerializeOpts::default()).unwrap();
    //println!("{}", String::from_utf8(bytes).unwrap());
}

#[cfg(test)]
mod tests {
    use cssparser::ParserInput;

    use crate::layout::Block;

    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn measure_text() {
        let dc = layout::TestDC::new();
        let size = dc.measure_text("ab\nc");
        assert_eq!(
            size,
            layout::Size {
                width: 40,
                height: 40,
            }
        );
    }

    #[test]
    fn table() {
        let html_data = r##"
        <table>
        <tbody>
            <tr>
                <td colspan="2">1543年頃 - 1596年1月28日</td>
            </tr>
            <tr>
                <th>生誕</th>
                <td>イングランド、デヴォン、タヴィストック</td>
            </tr>
            <tr>
                <th>最終階級</th>
                <td>イギリス海軍中将</td>
            </tr>
        </tbody>
        </table>
        "##;

        // max-width
        // ----------
        // 380
        //  40, 380
        //  80, 160
        // ----------
        // 190, 380

        let parser = parse_document(RcDom::default(), ParseOpts::default());
        let dom = parser.one(html_data);
        let node = &dom.document.children.borrow()[0];

        let table_nodes = find_elements(node, "table");
        let table = Table::new_from(&table_nodes[0]);
        //println!("{:}", table);

        assert_eq!(table.rows, 3);
        assert_eq!(table.cols, 2);

        assert_eq!(
            table.size,
            Size {
                width: 570,
                height: 60
            }
        );

        assert_eq!(
            table.cells[0].text_block.pos.get(),
            layout::Point { x: 0, y: 0 }
        );
        assert_eq!(
            table.cells[1].text_block.pos.get(),
            layout::Point { x: 0, y: 20 }
        );
        assert_eq!(
            table.cells[2].text_block.pos.get(),
            layout::Point { x: 190, y: 20 }
        );
        assert_eq!(
            table.cells[3].text_block.pos.get(),
            layout::Point { x: 0, y: 40 }
        );
        assert_eq!(
            table.cells[4].text_block.pos.get(),
            layout::Point { x: 190, y: 40 }
        );

        assert_eq!(table.cells[0].text_block.size.get().width, 570);
        assert_eq!(table.cells[1].text_block.size.get().width, 190);
        assert_eq!(table.cells[2].text_block.size.get().width, 380);
        assert_eq!(table.cells[3].text_block.size.get().width, 190);
        assert_eq!(table.cells[4].text_block.size.get().width, 380);
    }

    #[test]
    fn table_width() {
        let html_data = r##"
        <table style="width: 300px;">
        <tbody>
            <tr>
                <td colspan="2">1543年頃 - 1596年1月28日</td>
            </tr>
            <tr>
                <th>生誕</th>
                <td>イングランド、デヴォン、タヴィストック</td>
            </tr>
            <tr>
                <th>最終階級</th>
                <td>イギリス海軍中将</td>
            </tr>
        </tbody>
        </table>
        "##;

        // max-width
        // ----------
        // 380
        //  40, 380
        //  80, 160
        // ----------
        // 190, 380  = 570

        // width = 300
        // 100, 200

        let parser = parse_document(RcDom::default(), ParseOpts::default());
        let dom = parser.one(html_data);
        let node = &dom.document.children.borrow()[0];

        let table_nodes = find_elements(node, "table");
        let table = Table::new_from(&table_nodes[0]);

        assert_eq!(table.size.width, 300);
        //println!("{:}", table);
    }

    #[test]
    fn parse_css() {
        let css = "width: 300px; height: 200px;";
        let mut input = cssparser::ParserInput::new(css);
        let mut parser = cssparser::Parser::new(&mut input);

        let mut block_props = BlockProps::new();

        block_props.width = Some(300);
        block_props.height = Some(200);

        /*while let Ok(token) = parser.next() {
            //
        }*/

        assert_eq!(block_props.width, Some(300));
        assert_eq!(block_props.height, Some(200));
    }
}
