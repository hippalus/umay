import asyncio
import os
import signal
import sys
import websockets


class AsyncTCPEchoServer:
    def __init__(self, host, port, server_id):
        self.host = host
        self.port = port
        self.server_id = server_id
        self.clients = set()
        self.server = None

    async def handle_client(self, reader, writer):
        self.clients.add(writer)
        addr = writer.get_extra_info('peername')
        print(f"New TCP connection from {addr}")
        try:
            while True:
                data = await reader.read(1024)
                if not data:
                    break
                message = data.decode()
                response = f"Echo from TCP server {self.server_id} on port {self.port}: {message}"
                writer.write(response.encode())
                await writer.drain()
        except asyncio.CancelledError:
            pass
        finally:
            self.clients.remove(writer)
            writer.close()
            await writer.wait_closed()
            print(f"TCP connection closed for {addr}")

    async def run_server(self):
        self.server = await asyncio.start_server(
            self.handle_client, self.host, self.port)
        addr = self.server.sockets[0].getsockname()
        print(f'Serving TCP on {addr}')
        await self.server.serve_forever()

    async def shutdown(self):
        print(f"Shutting down the TCP server...")
        self.server.close()
        await self.server.wait_closed()
        for client in self.clients:
            client.close()
            await client.wait_closed()
        print("All TCP connections closed")


class AsyncWSEchoServer:
    def __init__(self, host, port, server_id):
        self.host = host
        self.port = port
        self.server_id = server_id
        self.server = None

    async def handle_client(self, websocket, path):
        addr = websocket.remote_address
        print(f"New WebSocket connection from {addr}")
        try:
            async for message in websocket:
                response = f"Echo from WS server {self.server_id} on port {self.port}: {message}"
                await websocket.send(response)
        except websockets.exceptions.ConnectionClosed:
            pass
        finally:
            print(f"WebSocket connection closed for {addr}")

    async def run_server(self):
        self.server = await websockets.serve(
            self.handle_client, self.host, self.port)
        print(f'Serving WebSocket on {self.host}:{self.port}')
        await self.server.wait_closed()

    async def shutdown(self):
        print(f"Shutting down the WebSocket server...")
        self.server.close()
        await self.server.wait_closed()
        print("All WebSocket connections closed")


async def main():
    host = '0.0.0.0'
    tcp_port = int(os.environ.get('TCP_PORT', 1994))
    ws_port = int(os.environ.get('WS_PORT', 1984))
    server_id = os.environ.get('SERVER_ID', 'Unknown')

    if len(sys.argv) > 2:
        tcp_port = int(sys.argv[1])
        ws_port = int(sys.argv[2])

    tcp_server = AsyncTCPEchoServer(host, tcp_port, f"{server_id}-TCP")
    ws_server = AsyncWSEchoServer(host, ws_port, f"{server_id}-WS")

    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, lambda: asyncio.create_task(tcp_server.shutdown()))
        loop.add_signal_handler(sig, lambda: asyncio.create_task(ws_server.shutdown()))

    try:
        await asyncio.gather(
            tcp_server.run_server(),
            ws_server.run_server()
        )
    except asyncio.CancelledError:
        pass
    finally:
        await asyncio.gather(
            tcp_server.shutdown(),
            ws_server.shutdown()
        )


if __name__ == "__main__":
    asyncio.run(main())
