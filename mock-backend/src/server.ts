import WebSocket from "ws";

const wss = new WebSocket.Server({
  port: 1233
});
wss.on("connection", (ws: WebSocket) => {
  console.log("New client connected");

  function loop() {
    setTimeout(() => {
      ws.send(JSON.stringify({ type: "timestamp", timestamp: Date.now() }));
      loop();
    }, 2000);
  }

  loop();

  ws.on("message", (message: string) => {
    console.log(`Received message: ${message}`);
  });

  ws.on("close", () => {
    console.log("Client disconnected");
  });
});

console.log("running");
