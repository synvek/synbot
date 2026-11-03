import { useChatSession } from '../components/chat-session/ChatSessionContext';
import { ChatSurface } from '../features/chat';

/**
 * The chat page is assembly and nothing else: the conversation lives in the shell
 * (`ChatSessionContext`), so switching to an admin page and back keeps the transcript, the
 * running turn, and the socket alive. Every pixel below is a presentational component under
 * `features/chat`.
 */
export default function Chat() {
  const {
    connected,
    running,
    messages,
    project,
    projects,
    planVersion,
    subagents,
    commands,
    send,
    cancel,
    approve,
    selectProject,
    closeProject,
  } = useChatSession();

  return (
    <ChatSurface
      connected={connected}
      running={running}
      messages={messages}
      project={project}
      projects={projects}
      planVersion={planVersion}
      subagents={subagents}
      commands={commands}
      onSend={send}
      onStop={cancel}
      onApprove={approve}
      onSelectProject={selectProject}
      onCloseProject={() => {
        if (project) void closeProject(project.id);
      }}
    />
  );
}
