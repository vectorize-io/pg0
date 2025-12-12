#!/bin/bash
set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

show_usage() {
    echo "Usage: ./release.sh <component> <version>"
    echo ""
    echo "Components:"
    echo "  cli     - CLI + Python package (tag: v*)"
    echo "  node    - Node.js client (tag: node-*)"
    echo ""
    echo "Version:"
    echo "  X.Y.Z   - Explicit version (e.g., 1.0.0)"
    echo "  patch   - Bump patch version (0.1.0 -> 0.1.1)"
    echo "  minor   - Bump minor version (0.1.0 -> 0.2.0)"
    echo "  major   - Bump major version (0.1.0 -> 1.0.0)"
    echo ""
    echo "Examples:"
    echo "  ./release.sh cli 0.1.0"
    echo "  ./release.sh cli patch"
    echo "  ./release.sh node 1.0.0"
    echo "  ./release.sh node patch"
    echo ""
    echo "Note: 'cli' releases both the CLI binaries and Python package together"
}

get_cli_version() {
    grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'
}

get_py_version() {
    grep '^version = ' sdk/python/pyproject.toml | head -1 | sed 's/version = "\(.*\)"/\1/'
}

get_node_version() {
    grep '"version"' sdk/node/package.json | head -1 | sed 's/.*"version": "\(.*\)".*/\1/'
}

bump_version() {
    local current=$1
    local bump_type=$2

    IFS='.' read -r MAJOR MINOR PATCH <<< "$current"

    case $bump_type in
        patch)
            PATCH=$((PATCH + 1))
            ;;
        minor)
            MINOR=$((MINOR + 1))
            PATCH=0
            ;;
        major)
            MAJOR=$((MAJOR + 1))
            MINOR=0
            PATCH=0
            ;;
    esac

    echo "${MAJOR}.${MINOR}.${PATCH}"
}

validate_version() {
    local version=$1
    if ! echo "$version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
        echo -e "${RED}Error: Invalid version format '$version'${NC}"
        echo "Version must be in semantic format: X.Y.Z (e.g., 1.0.0)"
        exit 1
    fi
}

check_clean_git() {
    if ! git diff --quiet || ! git diff --staged --quiet; then
        echo -e "${RED}Error: You have uncommitted changes. Please commit or stash them first.${NC}"
        git status --short
        exit 1
    fi
}

check_tag_exists() {
    local tag=$1
    if git rev-parse "$tag" >/dev/null 2>&1; then
        echo -e "${RED}Error: Tag $tag already exists${NC}"
        exit 1
    fi
}

release_cli() {
    local version=$1
    local current_cli=$(get_cli_version)
    local current_py=$(get_py_version)

    echo -e "${BLUE}CLI + Python Release${NC}"
    echo "Current CLI version: $current_cli"
    echo "Current Python version: $current_py"

    # Handle version bump (based on CLI version)
    if [ "$version" = "patch" ] || [ "$version" = "minor" ] || [ "$version" = "major" ]; then
        version=$(bump_version "$current_cli" "$version")
    fi

    validate_version "$version"
    local tag="v$version"

    check_clean_git
    check_tag_exists "$tag"

    echo -e "${YELLOW}Preparing release $tag (CLI + Python)${NC}"

    # Update version in Cargo.toml
    echo "Updating Cargo.toml version to $version..."
    sed -i.bak "s/^version = \".*\"/version = \"$version\"/" Cargo.toml
    rm -f Cargo.toml.bak

    # Update version in pyproject.toml
    echo "Updating pyproject.toml version to $version..."
    sed -i.bak "s/^version = \".*\"/version = \"$version\"/" sdk/python/pyproject.toml
    rm -f sdk/python/pyproject.toml.bak

    # Commit and tag
    git add Cargo.toml sdk/python/pyproject.toml
    git commit -m "chore: bump CLI version to $version"
    git tag -a "$tag" -m "Release $version"

    # Push
    git push
    git push origin "$tag"

    echo -e "${GREEN}Release $tag pushed!${NC}"
    echo ""
    echo "This will release:"
    echo "  - CLI binaries to GitHub Releases"
    echo "  - Python package to PyPI (pg0-embedded)"
}

release_node() {
    local version=$1
    local current=$(get_node_version)

    echo -e "${BLUE}Node.js Release${NC}"
    echo "Current version: $current"

    # Handle version bump
    if [ "$version" = "patch" ] || [ "$version" = "minor" ] || [ "$version" = "major" ]; then
        version=$(bump_version "$current" "$version")
    fi

    validate_version "$version"
    local tag="node-$version"

    check_clean_git
    check_tag_exists "$tag"

    echo -e "${YELLOW}Preparing Node.js release $tag${NC}"

    # Update version in package.json
    echo "Updating package.json version to $version..."
    cd sdk/node
    npm version "$version" --no-git-tag-version
    cd ../..

    # Commit and tag
    git add sdk/node/package.json
    git commit -m "chore: bump Node.js client version to $version"
    git tag -a "$tag" -m "Node.js Client Release $version"

    # Push
    git push
    git push origin "$tag"

    echo -e "${GREEN}Node.js release $tag pushed!${NC}"
    echo "Package will be published to npm as: @vectorize-io/pg0"
}

# Main
if [ -z "$1" ] || [ -z "$2" ]; then
    show_usage
    exit 1
fi

COMPONENT=$1
VERSION=$2

case $COMPONENT in
    cli)
        release_cli "$VERSION"
        ;;
    node|nodejs)
        release_node "$VERSION"
        ;;
    *)
        echo -e "${RED}Error: Unknown component '$COMPONENT'${NC}"
        echo ""
        show_usage
        exit 1
        ;;
esac
