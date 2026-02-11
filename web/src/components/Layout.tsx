import React from 'react'
import { Outlet } from 'react-router-dom'
import Sidebar from './Sidebar'
import Header from './Header'
import { ThemeProvider } from '../contexts/ThemeContext'

const Layout: React.FC = () => {
  return (
    <ThemeProvider>
      <div className="h-screen flex flex-col bg-background">
        <Header />
        <div className="flex flex-1 overflow-hidden">
          <Sidebar />
          <main className="flex-1 p-6 overflow-y-auto">
            <Outlet />
          </main>
        </div>
      </div>
    </ThemeProvider>
  )
}

export default Layout
