# Install
- recommend using venv `pip venv -m .venv`
- activate it: `.\.venv\Scripts\Activate.ps1`
- install dependencies: `pip install -r requirements.txt`

# Configure
- Config.py includes settings for server IP, for local only play set `SERVER = "localhost"`
- if you want to allow others to connect to your server you might need to adjust local firewall rules

# Run
- run `src/server.py` to start the server
- run `src/run.py` to start a client
- run `setup.py build` to create a redistributable executable for the current machines architecture

# Play
Currently not alot to do, but getting a base down to build on
- `<UP> <LEFT> <RIGHT> <DOWN>` to move
- `<SPACE>` to jump
- `Q` to change the terrain
- `E` to plant a tree
- `R` to build a solid structure (that you can jump onto)
- `<PLUS> <MINUS>` to zoom

# Game features
- procedural terrain - use perlin noise for terrain generation
- hot load all state - scenes, players, tiles, and decorations all load state direct from server
- authoritative server - everything that affects the game world is done by the server
- predictive rendering - we will try to render on client side, and confirm all actions from the server to keep it feeling native
- lag/latency correction - render position updates smoothly
- tile alignment - drift towards tile centers when moving
- freedom of movement - freely move and navigate the environment without tile locking
- yz-axis ordering - z-axis depth testing respects y-axis to get a 3D world in a simple isometric view
- hex tile based world - embraces hex tile superiority for mapping
