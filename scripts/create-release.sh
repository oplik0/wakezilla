#!/bin/bash

# Script to create a GitHub release
# Usage: ./scripts/create-release.sh [version]
# Example: ./scripts/create-release.sh 0.1.43

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to show help
show_help() {
    echo "Usage: $0 [version]"
    echo ""
    echo "Creates a Git tag and triggers the release workflow on GitHub."
    echo ""
    echo "Arguments:"
    echo "  version    Version in X.Y.Z format (e.g., 0.1.43)"
    echo ""
    echo "Examples:"
    echo "  $0 0.1.43"
    echo "  $0 1.0.0"
    echo ""
    echo "If version is not provided, it will be requested interactively."
}

# Check if we're in the correct directory
if [ ! -f "Cargo.toml" ]; then
    echo -e "${RED}Error: Run this script from the project root${NC}"
    exit 1
fi

# Check if git is available
if ! command -v git &> /dev/null; then
    echo -e "${RED}Error: git is not installed${NC}"
    exit 1
fi

# Check for uncommitted changes
if [ -n "$(git status --porcelain)" ]; then
    echo -e "${YELLOW}Warning: There are uncommitted changes in the repository${NC}"
    read -p "Do you want to continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Get version
if [ -z "$1" ]; then
    # Read current version from Cargo.toml
    CURRENT_VERSION=$(grep -m 1 "^version = " Cargo.toml | sed 's/version = "\(.*\)"/\1/')
    echo -e "${GREEN}Current version: ${CURRENT_VERSION}${NC}"
    read -p "Enter the new version (X.Y.Z format): " VERSION
else
    VERSION=$1
fi

# Validate version format
if [[ ! $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo -e "${RED}Error: Invalid version. Use X.Y.Z format (e.g., 0.1.43)${NC}"
    exit 1
fi

# Confirm action
echo ""
echo -e "${YELLOW}You are about to create release v${VERSION}${NC}"
echo ""
echo "This will:"
echo "  1. Create a Git tag: v${VERSION}"
echo "  2. Push the tag to GitHub"
echo "  3. Trigger the release workflow automatically"
echo ""
read -p "Do you want to continue? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Cancelled."
    exit 0
fi

# Update version in Cargo.toml
echo ""
echo -e "${GREEN}Updating version in Cargo.toml...${NC}"
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    sed -i '' "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
else
    # Linux
    sed -i "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
fi

# Verify the update worked
UPDATED_VERSION=$(grep -m 1 "^version = " Cargo.toml | sed 's/version = "\(.*\)"/\1/')
if [ "$UPDATED_VERSION" != "$VERSION" ]; then
    echo -e "${RED}Error: Failed to update version in Cargo.toml${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Version updated in Cargo.toml to ${VERSION}${NC}"

# Commit version change
echo ""
echo -e "${GREEN}Committing version update...${NC}"
git add Cargo.toml
git commit -m "chore: bump version to ${VERSION}" || {
    echo -e "${YELLOW}Warning: No changes to commit or commit failed${NC}"
}

# Create tag
echo ""
echo -e "${GREEN}Creating tag v${VERSION}...${NC}"
git tag -a "v${VERSION}" -m "Release v${VERSION}"

# Push
echo ""
echo -e "${GREEN}Pushing tag and commits...${NC}"
read -p "Do you want to push now? (Y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Nn]$ ]]; then
    git push origin main
    git push origin "v${VERSION}"
    echo ""
    echo -e "${GREEN}✓ Release v${VERSION} created successfully!${NC}"
    echo ""
    echo "The release workflow will run automatically on GitHub."
    echo "You can track progress at:"
    echo "  https://github.com/$(git config --get remote.origin.url | sed 's/.*github.com[:/]\(.*\)\.git/\1/')/actions"
else
    echo ""
    echo -e "${YELLOW}Tag created locally. To push, run:${NC}"
    echo "  git push origin main"
    echo "  git push origin v${VERSION}"
fi
