#!/bin/bash
#
# Helper script to set up GitHub secrets for PGQT release signing
#
# This script helps you configure the required secrets for macOS code signing
# and notarization. It does NOT store any secrets - it only guides you through
# the process of setting them in GitHub.
#
# Usage: ./.github/scripts/setup-secrets.sh
#

set -e

REPO="${1:-$(git remote get-url origin 2>/dev/null | sed 's/.*github.com[:/]//;s/\.git$//')}""

if [ -z "$REPO" ]; then
    echo "Error: Could not detect repository. Please run from a git repo or specify:"
    echo "  $0 owner/repo"
    exit 1
fi

echo "=========================================="
echo "PGQT GitHub Secrets Setup Helper"
echo "=========================================="
echo ""
echo "Repository: $REPO"
echo ""
echo "This script will help you set up the required secrets for:"
echo "  - macOS binary code signing"
echo "  - Apple notarization"
echo ""
echo "IMPORTANT: This script does NOT store or transmit any secrets."
echo "           It only provides guidance and commands to run."
echo ""

# Check if gh CLI is installed
if ! command -v gh &> /dev/null; then
    echo "⚠️  GitHub CLI (gh) not found. Install it from: https://cli.github.com/"
    echo ""
    echo "After installing, authenticate with: gh auth login"
    echo ""
fi

echo "=========================================="
echo "Step 1: Apple Developer Certificate"
echo "=========================================="
echo ""
echo "You need a Developer ID Application certificate from Apple."
echo ""
echo "To export your certificate:"
echo "  1. Open Keychain Access"
echo "  2. Find 'Developer ID Application: Your Name (TeamID)'"
echo "  3. Right-click → Export 'Developer ID Application...'"
echo "  4. Choose format: Personal Information Exchange (.p12)"
echo "  5. Set a password and remember it"
echo ""
echo "Then convert to base64:"
echo "  base64 -i ~/Downloads/DeveloperID.p12 | pbcopy"
echo ""

if command -v gh &> /dev/null; then
    echo "Command to set the secret:"
    echo "  gh secret set APPLE_CERTIFICATE_P12 --repo $REPO"
    echo ""
    echo "Paste the base64 output when prompted."
else
    echo "Go to: https://github.com/$REPO/settings/secrets/actions"
    echo "Add secret: APPLE_CERTIFICATE_P12 (paste base64 content)"
fi

echo ""
echo "=========================================="
echo "Step 2: Certificate Password"
echo "=========================================="
echo ""
echo "This is the password you set when exporting the .p12 file."
echo ""

if command -v gh &> /dev/null; then
    echo "Command to set the secret:"
    echo "  gh secret set APPLE_CERTIFICATE_PASSWORD --repo $REPO"
else
    echo "Go to: https://github.com/$REPO/settings/secrets/actions"
    echo "Add secret: APPLE_CERTIFICATE_PASSWORD"
fi

echo ""
echo "=========================================="
echo "Step 3: Apple ID"
echo "=========================================="
echo ""
echo "Your Apple ID email address (e.g., yourname@example.com)"
echo ""

if command -v gh &> /dev/null; then
    echo "Command to set the secret:"
    echo "  gh secret set APPLE_ID --repo $REPO"
else
    echo "Go to: https://github.com/$REPO/settings/secrets/actions"
    echo "Add secret: APPLE_ID"
fi

echo ""
echo "=========================================="
echo "Step 4: Apple App-Specific Password"
echo "=========================================="
echo ""
echo "You need an app-specific password for notarization:"
echo "  1. Go to https://appleid.apple.com"
echo "  2. Sign in → App-Specific Passwords"
echo "  3. Generate new password (e.g., 'PGQT-Notarization')"
echo "  4. Copy the generated password"
echo ""

if command -v gh &> /dev/null; then
    echo "Command to set the secret:"
    echo "  gh secret set APPLE_APP_PASSWORD --repo $REPO"
else
    echo "Go to: https://github.com/$REPO/settings/secrets/actions"
    echo "Add secret: APPLE_APP_PASSWORD"
fi

echo ""
echo "=========================================="
echo "Step 5: Apple Team ID"
echo "=========================================="
echo ""
echo "Your Apple Developer Team ID (10 characters, e.g., ABCDE12345)"
echo ""
echo "Find it at: https://developer.apple.com/account#MembershipDetailsCard"
echo ""

if command -v gh &> /dev/null; then
    echo "Command to set the secret:"
    echo "  gh secret set APPLE_TEAM_ID --repo $REPO"
else
    echo "Go to: https://github.com/$REPO/settings/secrets/actions"
    echo "Add secret: APPLE_TEAM_ID"
fi

echo ""
echo "=========================================="
echo "Summary"
echo "=========================================="
echo ""
echo "Required secrets:"
echo "  1. APPLE_CERTIFICATE_P12      - Base64-encoded .p12 certificate"
echo "  2. APPLE_CERTIFICATE_PASSWORD - Password for the certificate"
echo "  3. APPLE_ID                   - Your Apple ID email"
echo "  4. APPLE_APP_PASSWORD         - App-specific password"
echo "  5. APPLE_TEAM_ID              - Apple Developer Team ID"
echo ""

if command -v gh &> /dev/null; then
    echo "Quick setup with gh CLI:"
    echo ""
    echo "  # Certificate (paste base64 when prompted)"
    echo "  gh secret set APPLE_CERTIFICATE_P12 --repo $REPO"
    echo ""
    echo "  # Other secrets"
    echo "  gh secret set APPLE_CERTIFICATE_PASSWORD --repo $REPO"
    echo "  gh secret set APPLE_ID --repo $REPO"
    echo "  gh secret set APPLE_APP_PASSWORD --repo $REPO"
    echo "  gh secret set APPLE_TEAM_ID --repo $REPO"
    echo ""
    echo "Or use the web interface:"
    echo "  https://github.com/$REPO/settings/secrets/actions"
else
    echo "Set all secrets at:"
    echo "  https://github.com/$REPO/settings/secrets/actions"
fi

echo ""
echo "=========================================="
echo "Verification"
echo "=========================================="
echo ""

if command -v gh &> /dev/null; then
    echo "To verify secrets are set:"
    echo "  gh secret list --repo $REPO"
fi

echo ""
echo "To test the release workflow:"
echo "  1. Go to: https://github.com/$REPO/actions/workflows/release.yml"
echo "  2. Click 'Run workflow'"
echo "  3. Enter version tag (e.g., v0.1.0)"
echo "  4. Click 'Run workflow'"
echo ""
