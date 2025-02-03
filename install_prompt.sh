#!/bin/bash
# install_prompt.sh
# This script builds the prompt tool and creates a desktop launcher in Ubuntu.

set -e

# Build the release binary.
cargo build --release

# Get the full path to the built binary.
BIN_PATH="$(realpath target/release/prompt)"

# (Optional) Set an icon path.
# If you have an icon image, specify its absolute path here.
# If not, you can remove the Icon line in the desktop file.
ICON_PATH="$(realpath ~/code/prompt/icon.png)"

# Set the working directory (where your code and resources live).
WORKING_DIR="$(realpath ~/code/prompt)"

# Create the desktop entry directory if it doesn't exist.
DESKTOP_DIR="$HOME/.local/share/applications"
mkdir -p "$DESKTOP_DIR"

# Create the desktop file.
DESKTOP_FILE="$DESKTOP_DIR/prompt.desktop"
cat << EOF > "$DESKTOP_FILE"
[Desktop Entry]
Name=Prompt Generator
Comment=Generate prompts from your project files
Exec=$BIN_PATH
Path=$WORKING_DIR
Icon=$ICON_PATH
Terminal=false
Type=Application
Categories=Utility;
EOF

# Make the desktop file and binary executable.
chmod +x "$DESKTOP_FILE"
chmod +x "$BIN_PATH"

echo "Installation complete. You can now launch 'Prompt Generator' from your applications menu."
