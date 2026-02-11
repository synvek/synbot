import React from 'react'
import { useNavigate } from 'react-router-dom'
import { apiClient } from '../api/client'

const Header: React.FC = () => {
  const navigate = useNavigate()

  const handleLogout = () => {
    apiClient.clearAuth()
    navigate('/login')
  }

  return (
    <header className="bg-white shadow-sm">
      <div className="px-6 py-4 flex justify-between items-center">
        <h1 className="text-2xl font-bold text-gray-900">
          Web Admin Dashboard
        </h1>
        <button
          onClick={handleLogout}
          className="px-4 py-2 text-sm text-gray-700 hover:text-gray-900 hover:bg-gray-100 rounded-md transition-colors"
        >
          Logout
        </button>
      </div>
    </header>
  )
}

export default Header
