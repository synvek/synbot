import { lazy, Suspense } from 'react'
import { BrowserRouter, HashRouter, Routes, Route } from 'react-router-dom'
import { I18nProvider } from './i18n/I18nContext'
import { isDesktop } from './desktop/bridge'
import Layout from './components/Layout'
import ProtectedRoute from './components/ProtectedRoute'
import Login from './pages/Login'
import Overview from './pages/Overview'

// Overview and Login are the first paints; every other route is split so the entry chunk stays
// under the P6 budget and a rarely visited page never costs the chat workbench a byte.
const Channels = lazy(() => import('./pages/Channels'))
const Sessions = lazy(() => import('./pages/Sessions'))
const CronJobs = lazy(() => import('./pages/CronJobs'))
const Workflows = lazy(() => import('./pages/Workflows'))
const Roles = lazy(() => import('./pages/Roles'))
const Skills = lazy(() => import('./pages/Skills'))
const Plugins = lazy(() => import('./pages/Plugins'))
const Security = lazy(() => import('./pages/Security'))
const Config = lazy(() => import('./pages/Config'))
const Logs = lazy(() => import('./pages/Logs'))
const Chat = lazy(() => import('./pages/Chat'))

/** Route-level placeholder: a quiet bar where the page will land, never a spinner overlay. */
function RouteFallback() {
  return (
    <div className="flex h-full items-start justify-center pt-16">
      <div className="h-3 w-40 animate-pulse rounded-control bg-surface-2" aria-hidden="true" />
    </div>
  )
}

function App() {
  // The desktop app loads `index.html` from bundled assets, where nothing rewrites deep links to
  // the SPA, so a reload on `/chat` would 404. Hash routing keeps every route reloadable.
  const Router = isDesktop() ? HashRouter : BrowserRouter;

  return (
    <I18nProvider>
      <Router>
        <Suspense fallback={<RouteFallback />}>
          <Routes>
            <Route path="/login" element={<Login />} />
            <Route
              path="/"
              element={
                <ProtectedRoute>
                  <Layout />
                </ProtectedRoute>
              }
            >
              <Route index element={<Overview />} />
              <Route path="channels" element={<Channels />} />
              <Route path="sessions" element={<Sessions />} />
              <Route path="cron" element={<CronJobs />} />
              <Route path="workflows" element={<Workflows />} />
              <Route path="roles" element={<Roles />} />
              <Route path="skills" element={<Skills />} />
              <Route path="plugins" element={<Plugins />} />
              <Route path="security" element={<Security />} />
              <Route path="config" element={<Config />} />
              <Route path="logs" element={<Logs />} />
              <Route path="chat" element={<Chat />} />
            </Route>
          </Routes>
        </Suspense>
      </Router>
    </I18nProvider>
  )
}

export default App
