import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { I18nProvider } from './i18n/I18nContext'
import Layout from './components/Layout'
import ProtectedRoute from './components/ProtectedRoute'
import Login from './pages/Login'
import Overview from './pages/Overview'
import Channels from './pages/Channels'
import Sessions from './pages/Sessions'
import CronJobs from './pages/CronJobs'
import Roles from './pages/Roles'
import Skills from './pages/Skills'
import Config from './pages/Config'
import Logs from './pages/Logs'
import Chat from './pages/Chat'

function App() {
  return (
    <I18nProvider>
      <BrowserRouter>
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
            <Route path="roles" element={<Roles />} />
            <Route path="skills" element={<Skills />} />
            <Route path="config" element={<Config />} />
            <Route path="logs" element={<Logs />} />
            <Route path="chat" element={<Chat />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </I18nProvider>
  )
}

export default App
