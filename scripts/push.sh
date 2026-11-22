#!/usr/bin/env bash
# Push helper script for chess-hustler-mono

set -euo pipefail

REMOTE_NAME="${GIT_REMOTE:-origin}"
BRANCH_NAME="${GIT_BRANCH:-main}"

if [ $# -ge 1 ]; then
    TOKEN="$1"
    echo "Pushing using provided Personal Access Token..."
    git push "https://${TOKEN}@github.com/AnuragAmbuj/chess-hustler-mono.git" "${BRANCH_NAME}"
elif [ -n "${GITHUB_TOKEN:-}" ]; then
    echo "Pushing using GITHUB_TOKEN environment variable..."
    git push "https://${GITHUB_TOKEN}@github.com/AnuragAmbuj/chess-hustler-mono.git" "${BRANCH_NAME}"
elif [ -n "${GH_TOKEN:-}" ]; then
    echo "Pushing using GH_TOKEN environment variable..."
    git push "https://${GH_TOKEN}@github.com/AnuragAmbuj/chess-hustler-mono.git" "${BRANCH_NAME}"
else
    echo "Attempting git push with existing credentials..."
    git push "${REMOTE_NAME}" "${BRANCH_NAME}"
fi

echo "Push completed successfully."
