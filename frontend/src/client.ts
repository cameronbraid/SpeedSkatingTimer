import WebSocket from "ws";

let timestamps: number[] = [];

const ws = new WebSocket("ws://localhost:1233", {});

ws.on("open", () => {
  console.log("Connected to server");
});

ws.on("message", (message: string) => {
  JSON.parse(message);
});

ws.on("close", () => {
  console.log("Disconnected from server");
});
ws.on("error", (e) => {
  console.error(`Error: ${e}`);
});
