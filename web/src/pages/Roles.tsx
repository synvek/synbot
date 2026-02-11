import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { RoleInfo } from '../types/api'

const Roles: React.FC = () => {
  const [roles, setRoles] = useState<RoleInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [selectedRole, setSelectedRole] = useState<RoleInfo | null>(null)

  useEffect(() => {
    const fetchRoles = async () => {
      try {
        setLoading(true)
        const data = await apiClient.getRoles()
        setRoles(data)
        setError(null)
      } catch (err) {
        setError('Failed to fetch roles')
        console.error(err)
      } finally {
        setLoading(false)
      }
    }

    fetchRoles()
  }, [])

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-gray-500">Loading...</div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="bg-red-50 border border-red-200 rounded-lg p-4">
        <p className="text-red-800">{error}</p>
      </div>
    )
  }

  return (
    <div>
      <div className="mb-6">
        <h2 className="text-2xl font-bold text-gray-900">Roles</h2>
        <p className="text-gray-600 mt-1">Agent role configurations</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="space-y-4">
          {roles.map((role) => (
            <div
              key={role.name}
              onClick={() => setSelectedRole(role)}
              className={`bg-white rounded-lg shadow p-4 cursor-pointer transition-all hover:shadow-lg ${
                selectedRole?.name === role.name ? 'ring-2 ring-blue-500' : ''
              }`}
            >
              <h3 className="text-lg font-semibold text-gray-900">{role.name}</h3>
              <div className="mt-2 space-y-1 text-sm text-gray-600">
                <p>Model: {role.provider}/{role.model}</p>
                <p>Skills: {role.skills.length}</p>
                <p>Tools: {role.tools.length}</p>
              </div>
            </div>
          ))}
        </div>

        {selectedRole && (
          <div className="bg-white rounded-lg shadow p-6">
            <h3 className="text-xl font-bold text-gray-900 mb-4">
              {selectedRole.name}
            </h3>

            <div className="space-y-4">
              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">System Prompt</h4>
                <div className="bg-gray-50 rounded p-3 text-sm text-gray-800 whitespace-pre-wrap max-h-48 overflow-y-auto">
                  {selectedRole.system_prompt}
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">Model Configuration</h4>
                <div className="bg-gray-50 rounded p-3 space-y-1 text-sm">
                  <p><span className="font-medium">Provider:</span> {selectedRole.provider}</p>
                  <p><span className="font-medium">Model:</span> {selectedRole.model}</p>
                  <p><span className="font-medium">Max Tokens:</span> {selectedRole.max_tokens}</p>
                  <p><span className="font-medium">Temperature:</span> {selectedRole.temperature}</p>
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">Skills</h4>
                <div className="flex flex-wrap gap-2">
                  {selectedRole.skills.map((skill) => (
                    <span
                      key={skill}
                      className="px-3 py-1 bg-blue-100 text-blue-800 rounded-full text-sm"
                    >
                      {skill}
                    </span>
                  ))}
                  {selectedRole.skills.length === 0 && (
                    <span className="text-gray-500 text-sm">No skills assigned</span>
                  )}
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">Tools</h4>
                <div className="flex flex-wrap gap-2">
                  {selectedRole.tools.map((tool) => (
                    <span
                      key={tool}
                      className="px-3 py-1 bg-green-100 text-green-800 rounded-full text-sm"
                    >
                      {tool}
                    </span>
                  ))}
                  {selectedRole.tools.length === 0 && (
                    <span className="text-gray-500 text-sm">No tools assigned</span>
                  )}
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">Workspace</h4>
                <code className="block bg-gray-50 rounded p-3 text-sm text-gray-800">
                  {selectedRole.workspace_dir}
                </code>
              </div>
            </div>
          </div>
        )}
      </div>

      {roles.length === 0 && (
        <div className="text-center py-12 text-gray-500">
          No roles configured
        </div>
      )}
    </div>
  )
}

export default Roles
