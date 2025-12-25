import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react-swc'
import { tanstackRouter } from '@tanstack/router-plugin/vite'
import path from 'path'
import fs from 'fs'

// Plugin to copy frontend JWT file from .nats-creds to assets folder
const copyFrontendJwt = () => {
  return {
    name: 'copy-frontend-jwt',
    buildStart() {
      const sourcePath = path.resolve(__dirname, '../.nats-creds/frontend.jwt')
      const destPath = path.resolve(__dirname, 'src/assets/frontend.jwt')
      
      // Ensure assets directory exists
      const assetsDir = path.dirname(destPath)
      if (!fs.existsSync(assetsDir)) {
        fs.mkdirSync(assetsDir, { recursive: true })
      }
      
      // Copy file if it exists
      if (fs.existsSync(sourcePath)) {
        fs.copyFileSync(sourcePath, destPath)
        console.log(`Copied frontend JWT from ${sourcePath} to ${destPath}`)
      } else {
        // Create empty file if source doesn't exist (for dev without nats.sh)
        if (!fs.existsSync(destPath)) {
          fs.writeFileSync(destPath, '')
          console.log(`Created empty frontend JWT file at ${destPath}`)
        }
      }
    },
  }
}

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    copyFrontendJwt(),
    tanstackRouter({
      target: 'react',
      autoCodeSplitting: true,
      routesDirectory: './src/routes',
      generatedRouteTree: './src/routeTree.gen.ts',
    }),
    react(),
  ],
  resolve: {
    alias: {
      'react-7-segment-display': path.resolve(__dirname, 'node_modules/react-7-segment-display/src'),
    },
  },
  server: {
    hmr: {
      // When accessed through the backend proxy (port 8001), 
      // tell HMR client to connect directly to Vite for WebSocket
      clientPort: 5173,
    },
  },
})
