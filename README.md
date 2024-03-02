# Install
- recommend using venv `pip venv -m .venv`
- activate it: `.\.venv\Scripts\Activate.ps1`
- install dependencies: `pip install -r requirements.txt`

# Configure
- Config.py includes settings for server IP, for local only play set `SERVER = "localhost"`
- if you want to allow others to connect to your server you might need to adjust local firewall rules

# Run
- from `src`, run `server.py` to start the server
- run `src`, run `run.py` to start a client
- run `setup.py build` to create a redistributable executable for the current machines architecture

# Play
Currently not alot to do, but getting a base down to build on
- `Q` to change the focused tile
- `E` to plant a tree
- `R` to build a solid structure
- `<SPACE>` to jump

# Technical Features
- authoritative server - everything that affects the game world is done by the server
- predictive rendering - we will try to render on client side, and confirm all actions from the server to keep it feeling native
- lag/latency correction - render position updates smoothly

# Game features
- hex tile based world - embraces hex tile superiority for mapping, think outside the box
- freedom of movement, tile-based dynamics - freely move and navigate the environment, but interaction remains tile-based
- tile alignment - drift towards tile centers when moving
- yz-axis ordering - z-axis depth testing respects y-axis for to get a 3D world in a simple isometric view
- hot load all state - players, tiles, and decorations all load state direct from server, and rendered with local assets
