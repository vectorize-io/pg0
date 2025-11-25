#!/bin/bash
set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')

echo "Current version: $CURRENT_VERSION"
echo ""

# Check if version argument provided
if [ -z "$1" ]; then
    echo "Usage: ./release.sh <version>"
    echo ""
    echo "Examples:"
    echo "  ./release.sh 0.1.0"
    echo "  ./release.sh 1.0.0"
    echo "  ./release.sh patch   # Bump patch version (0.1.0 -> 0.1.1)"
    echo "  ./release.sh minor   # Bump minor version (0.1.0 -> 0.2.0)"
    echo "  ./release.sh major   # Bump major version (0.1.0 -> 1.0.0)"
    exit 1
fi

VERSION=$1

# Handle semantic version bumps
if [ "$VERSION" = "patch" ] || [ "$VERSION" = "minor" ] || [ "$VERSION" = "major" ]; then
    IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

    case $VERSION in
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

    VERSION="${MAJOR}.${MINOR}.${PATCH}"
fi

# Validate semantic version format
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo -e "${RED}Error: Invalid version format '$VERSION'${NC}"
    echo "Version must be in semantic format: X.Y.Z (e.g., 1.0.0)"
    exit 1
fi

TAG="v$VERSION"

echo -e "${YELLOW}Preparing release $TAG${NC}"
echo ""

# Check for uncommitted changes
if ! git diff --quiet || ! git diff --staged --quiet; then
    echo -e "${RED}Error: You have uncommitted changes. Please commit or stash them first.${NC}"
    git status --short
    exit 1
fi

# Check if tag already exists
if git rev-parse "$TAG" >/dev/null 2>&1; then
    echo -e "${RED}Error: Tag $TAG already exists${NC}"
    exit 1
fi

# Update version in Cargo.toml
echo "Updating Cargo.toml version to $VERSION..."
sed -i.bak "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
rm -f Cargo.toml.bak

# Update Cargo.lock
echo "Updating Cargo.lock..."
cargo check --quiet 2>/dev/null || true

# Commit the version bump
echo "Committing version bump..."
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to $VERSION"

# Create and push tag
echo "Creating tag $TAG..."
git tag -a "$TAG" -m "Release $VERSION"

echo ""
echo -e "${GREEN}Ready to release!${NC}"
echo ""
echo "To push the release, run:"
echo -e "  ${YELLOW}git push && git push origin $TAG${NC}"
echo ""
echo "This will trigger the GitHub Actions release workflow."
