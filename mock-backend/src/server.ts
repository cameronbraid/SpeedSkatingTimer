import WebSocket from "ws";

const wss = new WebSocket.Server({
  port: 1233
});
wss.on("connection", (ws: WebSocket) => {
  console.log("New client connected");

  
  function loop_send_timestamp() {
    setTimeout(() => {
      ws.send(JSON.stringify({ type: "timestamp", timestamp: Date.now(), duration: 2000000000 }));
      loop_send_timestamp();
    }, 2000);
  }

    
  let connected = false;
  function loop_send_setup() {
    setTimeout(() => {
      connected = !connected
      ws.send(JSON.stringify({ type: "setup", connected }));
      loop_send_setup();
    }, 1000 +(Math.random() * 1000));
  }

  loop_send_timestamp();
  
  loop_send_setup();

  ws.on("message", (message: string) => {
    console.log(`Received message: ${message}`);
  });

  ws.on("close", () => {
    console.log("Client disconnected");
  });
});

console.log("Listening on port 1233");
