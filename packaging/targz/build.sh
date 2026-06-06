#!/bin/bash
set -e

APP_NAME="tundracode"
VERSION="0.1.0"
BUILD_DIR="target/release"
DIST_DIR="dist"
BINARY_NAME="tundracode"

echo "Empaquetando TundraCode v${VERSION}..."

cargo build --release

rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}/${APP_NAME}"

cp "${BUILD_DIR}/${BINARY_NAME}" "${DIST_DIR}/${APP_NAME}/"

if [ -d "src" ]; then
    cp -r src "${DIST_DIR}/${APP_NAME}/"
fi

if [ -d "assets" ]; then
    cp -r assets "${DIST_DIR}/${APP_NAME}/"
fi

cat > "${DIST_DIR}/${APP_NAME}/run.sh" << 'EOF'
#!/bin/bash
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "${DIR}/tundracode" "$@"
EOF
chmod +x "${DIST_DIR}/${APP_NAME}/run.sh"

cd "${DIST_DIR}"
tar czf "${APP_NAME}-${VERSION}-linux-x86_64.tar.gz" "${APP_NAME}"
cd ..

echo "Empaquetado completado: ${DIST_DIR}/${APP_NAME}-${VERSION}-linux-x86_64.tar.gz"
