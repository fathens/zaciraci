use crate::ref_finance::path::edge::EdgeWeight;
use crate::ref_finance::path::graph::CachedPath;
use crate::ref_finance::pool_info::{PoolInfo, PoolInfoList, TokenPairId};
use petgraph::Graph;
use petgraph::algo::dijkstra;
use petgraph::graph::NodeIndex;
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::Add;
use std::panic;
use std::sync::Arc;

#[derive(Default, PartialOrd, Eq, Hash, Copy, Clone)]
struct Edge<'a> {
    i: &'a str,
    o: &'a str,

    weight: u32,
}

impl Add<Edge<'_>> for Edge<'_> {
    type Output = Self;

    fn add(self, rhs: Edge<'_>) -> Self::Output {
        Self {
            i: "",
            o: "",
            weight: self.weight + rhs.weight,
        }
    }
}

impl Debug for Edge<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}-> {}", self.i, self.weight, self.o)
    }
}

impl PartialEq<Self> for Edge<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.weight == other.weight
    }
}

impl Ord for Edge<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.weight.cmp(&other.weight)
    }
}

#[test]
fn test_update_path() {
    let mut graph = Graph::new();
    let mut nodes = HashMap::new();
    fn add_node(
        graph: &mut Graph<&'static str, Edge>,
        nodes: &mut HashMap<&'static str, NodeIndex>,
        i: &'static str,
        o: &'static str,
        weight_io: u32,
        weight_oi: u32,
    ) {
        let &mut node_i = nodes.entry(i).or_insert_with(|| graph.add_node(i));
        let &mut node_o = nodes.entry(o).or_insert_with(|| graph.add_node(o));
        graph.add_edge(
            node_i,
            node_o,
            Edge {
                i,
                o,
                weight: weight_io,
            },
        );
        graph.add_edge(
            node_o,
            node_i,
            Edge {
                i: o,
                o: i,
                weight: weight_oi,
            },
        );
    }

    //  A --1|2-- B
    //  |         |
    // 3|2       4|5
    //  |         |
    //  C --4|3-- D
    //  |         |
    // 6|7       8|9
    //  |         |
    //  E --5|6-- F

    // 往路
    // A 1-> B 4-> D 8-> F = 13
    // A 3-> C 6-> E 5-> F = 14
    // A 3-> C 4-> D 8-> F = 15

    // 復路
    // F 9-> D 3-> C 2-> A = 14
    // F 6-> E 7-> C 2-> A = 15
    // F 9-> D 5-> B 2-> A = 16

    add_node(&mut graph, &mut nodes, "A", "B", 1, 2);
    add_node(&mut graph, &mut nodes, "A", "C", 3, 2);
    add_node(&mut graph, &mut nodes, "B", "D", 4, 5);
    add_node(&mut graph, &mut nodes, "C", "D", 4, 3);
    add_node(&mut graph, &mut nodes, "C", "E", 6, 7);
    add_node(&mut graph, &mut nodes, "D", "F", 8, 9);
    add_node(&mut graph, &mut nodes, "E", "F", 5, 6);

    assert_eq!(nodes.len(), 6);
    assert!(nodes.contains_key("A"));

    let cached_path = CachedPath::new(graph, nodes);

    match panic::catch_unwind(|| cached_path.update_path(&"X", None)) {
        Err(e) => panic!("something wrong: {:?}", e),
        Ok(Ok(v)) => panic!("should error: {:?}", v),
        Ok(Err(e)) => {
            let msg = format!("{}", e);
            assert_eq!(msg, "Cannot find token account: X");
        }
    }
    match panic::catch_unwind(|| cached_path.update_path(&"A", Some("X"))) {
        Err(e) => panic!("something wrong: {:?}", e),
        Ok(Ok(v)) => panic!("should error: {:?}", v),
        Ok(Err(e)) => {
            let msg = format!("{}", e);
            assert_eq!(msg, "Cannot find token account: X");
        }
    }
    let goals = cached_path.update_path(&"A", None).unwrap();
    assert_eq!(goals.len(), 5);
    for goal in goals.into_iter() {
        let gs = cached_path.update_path(&goal, Some("A")).unwrap();
        assert!(gs.len() < 6);
        assert!(!gs.is_empty());
    }

    // A <-> B
    assert_eq!(
        format!("{:?}", cached_path.get_edges(&"A", &"B").unwrap()),
        "[A 1-> B]"
    );
    assert_eq!(
        format!("{:?}", cached_path.get_edges(&"B", &"A").unwrap()),
        "[B 2-> A]"
    );

    // A <-> C
    assert_eq!(
        format!("{:?}", cached_path.get_edges(&"A", &"C").unwrap()),
        "[A 3-> C]"
    );
    assert_eq!(
        format!("{:?}", cached_path.get_edges(&"C", &"A").unwrap()),
        "[C 2-> A]"
    );

    // A <-> D
    assert_eq!(
        format!("{:?}", cached_path.get_edges(&"A", &"D").unwrap()),
        "[A 1-> B, B 4-> D]"
    );
    assert_eq!(
        format!("{:?}", cached_path.get_edges(&"D", &"A").unwrap()),
        "[D 3-> C, C 2-> A]"
    );

    // A <-> E
    assert_eq!(
        format!("{:?}", cached_path.get_edges(&"A", &"E").unwrap()),
        "[A 3-> C, C 6-> E]"
    );
    assert_eq!(
        format!("{:?}", cached_path.get_edges(&"E", &"A").unwrap()),
        "[E 7-> C, C 2-> A]"
    );

    // A <-> F
    assert_eq!(
        format!("{:?}", cached_path.get_edges(&"A", &"F").unwrap()),
        "[A 1-> B, B 4-> D, D 8-> F]"
    );
    assert_eq!(
        format!("{:?}", cached_path.get_edges(&"F", &"A").unwrap()),
        "[F 9-> D, D 3-> C, C 2-> A]"
    );
}

#[test]
fn test_find_all_path() {
    //     B --2-- C
    //     |       |
    //     3       2
    //     |       |
    //     D       E
    //     |       |
    //     4       7
    //      \     /
    //       F-2-G
    //       |  |
    //       1  3
    //       |  |
    //       H  I
    //       |  |
    //       6  5
    //        \ |
    //         J
    //         |
    //         2
    //         |
    //         A
    let mut graph = Graph::new();
    let a = graph.add_node("A");
    let b = graph.add_node("B");
    let c = graph.add_node("C");
    let d = graph.add_node("D");
    let e = graph.add_node("E");
    let f = graph.add_node("F");
    let g = graph.add_node("G");
    let h = graph.add_node("H");
    let i = graph.add_node("I");
    let j = graph.add_node("J");

    graph.add_edge(c, b, 2);
    graph.add_edge(d, b, 3);
    graph.add_edge(e, c, 2);
    graph.add_edge(f, d, 4);
    graph.add_edge(g, e, 7);
    graph.add_edge(g, f, 2);
    graph.add_edge(h, f, 1);
    graph.add_edge(i, g, 3);
    graph.add_edge(j, h, 6);
    graph.add_edge(j, i, 5);
    graph.add_edge(a, j, 2);

    let goals = dijkstra(&graph, a, None, |e| *e.weight());
    assert_eq!(goals.len(), 10);

    let finder = super::GraphPath {
        graph: &graph,
        goals: &goals,
    };
    let mut results = finder.find_all_path();
    assert_eq!(results.len(), 9);
    results.sort();

    assert_eq!(
        results,
        vec![
            vec!["J"],
            vec!["J", "H"],
            vec!["J", "H", "F"],
            vec!["J", "H", "F", "D"],
            vec!["J", "H", "F", "D", "B"],
            vec!["J", "I"],
            vec!["J", "I", "G"],
            vec!["J", "I", "G", "E"],
            vec!["J", "I", "G", "E", "C"],
        ]
    );
}

#[test]
fn test_find_all_path_looped() {
    fn weight(v: u8) -> EdgeWeight {
        EdgeWeight::new(
            TokenPairId {
                pool_id: 0,
                token_in: 0.into(),
                token_out: 0.into(),
            },
            1,
            v as u128,
        )
    }
    //  B-1-C
    //  |   |
    //  2   2
    //   \ /
    //    A
    let mut graph = Graph::new();
    let a = graph.add_node("A");
    let b = graph.add_node("B");
    let c = graph.add_node("C");

    graph.add_edge(a, b, weight(2));
    graph.add_edge(a, c, weight(2));
    graph.add_edge(b, c, weight(1));
    graph.add_edge(c, b, weight(1));

    let goals = dijkstra(&graph, a, None, |e| *e.weight());
    assert_eq!(goals.len(), 3);

    let finder = super::GraphPath {
        graph: &graph,
        goals: &goals,
    };
    let mut results = finder.find_all_path();
    assert_eq!(results.len(), 2);
    results.sort();

    assert_eq!(results, vec![vec!["B"], vec!["C"]]);
}

#[test]
fn test_with_sample_pools() {
    let json_str = r#"[
  {
    "id": 1230,
    "timestamp": "2025-04-09T12:39:33.059132",
    "bare": {
      "pool_kind": "SIMPLE_POOL",
      "token_account_ids": [
        "nearpunk.tkn.near",
        "nearkat.tkn.near"
      ],
      "amounts": [
        "917901265701983007",
        "32722705099615176853137739174"
      ],
      "total_fee": 30,
      "shares_total_supply": "168559907784268610600",
      "amp": 0
    }
  },
  {
    "id": 1233,
    "timestamp": "2025-04-09T12:39:33.059132",
    "bare": {
      "pool_kind": "SIMPLE_POOL",
      "token_account_ids": [
        "neardog.tkn.near",
        "nearkat.tkn.near"
      ],
      "amounts": [
        "112549664748034977674143002",
        "379117201618630649762"
      ],
      "total_fee": 30,
      "shares_total_supply": "201313109557693733814",
      "amp": 0
    }
  },
  {
    "id": 1236,
    "timestamp": "2025-04-09T12:39:33.059133",
    "bare": {
      "pool_kind": "SIMPLE_POOL",
      "token_account_ids": [
        "hak.tkn.near",
        "neardog.tkn.near"
      ],
      "amounts": [
        "47979068235102569424584",
        "2210084875460890521352625"
      ],
      "total_fee": 30,
      "shares_total_supply": "1065484599128218",
      "amp": 0
    }
  },
  {
    "id": 1238,
    "timestamp": "2025-04-09T12:39:33.059133",
    "bare": {
      "pool_kind": "SIMPLE_POOL",
      "token_account_ids": [
        "hak.tkn.near",
        "nearkat.tkn.near"
      ],
      "amounts": [
        "6391367452222673233661824",
        "10662016351996707616953347"
      ],
      "total_fee": 30,
      "shares_total_supply": "25438534349775842",
      "amp": 0
    }
  },
  {
    "id": 1302,
    "timestamp": "2025-04-09T12:39:33.059139",
    "bare": {
      "pool_kind": "SIMPLE_POOL",
      "token_account_ids": [
        "meritocracy.tkn.near",
        "hak.tkn.near"
      ],
      "amounts": [
        "1852766544899218236739",
        "56195909476386860720332"
      ],
      "total_fee": 30,
      "shares_total_supply": "1003519598254699325699818",
      "amp": 0
    }
  },
  {
    "id": 1903,
    "timestamp": "2025-04-09T12:39:33.059198",
    "bare": {
      "pool_kind": "SIMPLE_POOL",
      "token_account_ids": [
        "meritocracy.tkn.near",
        "meta-token.near"
      ],
      "amounts": [
        "2752957070444978844861",
        "1717994639306174656160"
      ],
      "total_fee": 60,
      "shares_total_supply": "427714971472454349119456",
      "amp": 0
    }
  },
  {
    "id": 3805,
    "timestamp": "2025-04-09T12:39:33.059384",
    "bare": {
      "pool_kind": "SIMPLE_POOL",
      "token_account_ids": [
        "ftv2.nekotoken.near",
        "meta-token.near"
      ],
      "amounts": [
        "27204830623115822689561871518",
        "103768683992951076017185176"
      ],
      "total_fee": 60,
      "shares_total_supply": "130381242312197246928404",
      "amp": 0
    }
  },
  {
    "id": 3820,
    "timestamp": "2025-04-09T12:39:33.059386",
    "bare": {
      "pool_kind": "SIMPLE_POOL",
      "token_account_ids": [
        "ftv2.nekotoken.near",
        "nexp.near"
      ],
      "amounts": [
        "186392767880307151007148371",
        "1850875"
      ],
      "total_fee": 60,
      "shares_total_supply": "176335161076675344496556183",
      "amp": 0
    }
  },
  {
    "id": 4421,
    "timestamp": "2025-04-09T12:39:33.059445",
    "bare": {
      "pool_kind": "SIMPLE_POOL",
      "token_account_ids": [
        "wrap.near",
        "nearpunk.tkn.near"
      ],
      "amounts": [
        "32554286246618058848759",
        "824746999920770719130193812211698363"
      ],
      "total_fee": 1900,
      "shares_total_supply": "139296537538051832583095",
      "amp": 0
    }
  }
]"#;
    let pools: Vec<PoolInfo> = serde_json::from_slice(json_str.as_bytes()).unwrap();
    assert_eq!(pools.len(), 9);
    let pools = pools.into_iter().map(Arc::new).collect();
    let _pools_list = Arc::new(PoolInfoList::new(pools));
}
