use std::collections::HashMap;
use std::collections::HashSet;
use std::net::SocketAddr;

use parking_lot::RwLock;
use serde::Deserialize;
use std::sync::Arc;

const RPC_PORT: u16 = 3030;

#[derive(Deserialize, Debug)]
struct PeerInfoResponse {
    jsonrpc: String,
    id: String,
    result: PeerInfo,
}

#[derive(Deserialize, Debug)]
struct PeerInfo {
    peers: Vec<SocketAddr>,
}

#[tokio::main]
async fn main() {
    let initial_node: SocketAddr = "50.18.246.201:4131".parse().unwrap();

    let nodes: Arc<RwLock<HashMap<SocketAddr, HashSet<SocketAddr>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    nodes.write().insert(initial_node, HashSet::new());

    // curl --data-binary '{"jsonrpc": "2.0", "id":"documentation", "method": "getpeerinfo", "params": [] }' -H 'content-type: application/json' http://127.0.0.1:3030/
    let data_peers =
        r#"{"jsonrpc": "2.0", "id":"documentation", "method": "getpeerinfo", "params": []}"#;

    loop {
        dbg!(&nodes.read());
        // Copy the addresses for this iteration.
        let addrs: HashSet<SocketAddr> = nodes.read().keys().cloned().collect();

        for addr in addrs {
            let nodes_clone = nodes.clone();
            tokio::task::spawn(async move {
                let mut rpc = addr;
                rpc.set_port(RPC_PORT);

                // Query the node for peers.
                let client = reqwest::Client::new();
                let peer_info_res = client
                    .post(format!("http://{}", rpc.clone()))
                    .timeout(std::time::Duration::from_secs(5))
                    .body(data_peers)
                    .header("content-type", "application/json")
                    .send()
                    .await;

                let peer_info_response = match peer_info_res {
                    Err(_err) => return,
                    Ok(res) => res.json::<PeerInfoResponse>().await.unwrap(),
                };

                // Update the list of peers for this node.
                let result_peers = peer_info_response.result.peers.iter();
                let new_peers = result_peers.clone().cloned().collect();
                nodes_clone.write().insert(addr, new_peers);

                // Insert new address to be queried in the next loop.
                for addr in result_peers {
                    nodes_clone.write().insert(*addr, HashSet::new());
                }
            });
        }

        // Sleep for 10s between crawls.
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}
