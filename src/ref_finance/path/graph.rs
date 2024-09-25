#[cfg(test)]
mod test {
    use crate::logging::*;
    use petgraph::algo::dijkstra;
    use petgraph::Graph;
    use std::thread::sleep;

    fn sample() {
        let log = DEFAULT.new(o!(
            "function" => "sample",
        ));
        info!(log, "start");

        let mut graph = Graph::new();
        let a = graph.add_node("A");
        let b = graph.add_node("B");
        let c = graph.add_node("C");
        let d = graph.add_node("D");
        let e = graph.add_node("E");
        let f = graph.add_node("F");

        graph.add_edge(a, b, 7);
        graph.add_edge(a, c, 9);
        graph.add_edge(a, f, 14);
        graph.add_edge(b, c, 10);
        graph.add_edge(b, d, 15);
        graph.add_edge(c, d, 11);
        graph.add_edge(c, f, 2);
        graph.add_edge(d, e, 6);
        graph.add_edge(e, f, 9);

        let path = dijkstra(&graph, a, None, |e| *e.weight());
        info!(log, "result"; "size" => path.len());
        for (node, weight) in path {
            info!(log, "node"; "node" => graph[node], "weight" => weight);
        }
    }

    #[test]
    fn test_sample() {
        sample();
        sleep(std::time::Duration::from_secs(1));
    }
}
