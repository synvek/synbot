import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App.tsx'
import './index.css'
import { apiClient } from './api/client'
import { bootstrapDesktop, isDesktop } from './desktop/bridge'
import StartupFailure from './desktop/StartupFailure'
import StartupSplash from './desktop/StartupSplash'

const root = ReactDOM.createRoot(document.getElementById('root')!)

/**
 * The desktop shell owns the daemon address and its credentials, so the bridge has to run before
 * the first render — otherwise `ProtectedRoute` would race the bootstrap and bounce through a
 * login form that the desktop app does not have. A splash covers the wait on the desktop; the
 * web build resolves instantly and never shows it.
 */
async function start() {
  if (isDesktop()) root.render(<StartupSplash />)
  try {
    const session = await bootstrapDesktop()
    if (session) {
      apiClient.configureDesktop(session)
    } else {
      // Web build: fall back to the Basic credentials stored by the login page.
      apiClient.loadAuth()
    }
    root.render(
      <React.StrictMode>
        <App />
      </React.StrictMode>
    )
  } catch (error) {
    root.render(
      <StartupFailure message={error instanceof Error ? error.message : String(error)} />
    )
  }
}

void start()
