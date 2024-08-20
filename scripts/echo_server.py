import asyncio
import os
import signal
import sys


class AsyncTCPEchoServer:
    def __init__(self, host, port, server_id):
        self.host = host
        self.port = port
        self.server_id = server_id
        self.clients = set()

    async def handle_client(self, reader, writer):
        self.clients.add(writer)
        addr = writer.get_extra_info('peername')
        print(f"New connection from {addr}")
        try:
            while True:
                data = await reader.read(1024)
                if not data:
                    break
                message = data.decode()
                response = f"Echo from server {self.server_id} on port {self.port}: {message}"
                writer.write(response.encode())
                await writer.drain()
        except asyncio.CancelledError:
            pass
        finally:
            self.clients.remove(writer)
            writer.close()
            await writer.wait_closed()
            print(f"Connection closed for {addr}")

    async def run_server(self):
        server = await asyncio.start_server(
            self.handle_client, self.host, self.port)

        addr = server.sockets[0].getsockname()
        print(f'Serving on {addr}')

        async with server:
            await server.serve_forever()

    async def shutdown(self):
        print("Shutting down the server...")
        for client in self.clients:
            client.close()
        await asyncio.gather(*[client.wait_closed() for client in self.clients])
        print("All connections closed")


async def main():
    host = '0.0.0.0'
    port = int(os.environ.get('PORT', 1994))
    server_id = os.environ.get('SERVER_ID', 'Unknown')

    if len(sys.argv) > 1:
        port = int(sys.argv[1])

    server = AsyncTCPEchoServer(host, port, server_id)

    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, lambda: asyncio.create_task(server.shutdown()))

    try:
        await server.run_server()
    finally:
        await server.shutdown()


if __name__ == "__main__":
    asyncio.run(main())
