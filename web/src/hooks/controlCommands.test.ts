import { describe, expect, it } from 'vitest';
import { endsTurn, isControlCommand } from './controlCommands';

describe('control command classification', () => {
  it('recognises the built-in commands the daemon answers itself', () => {
    for (const command of [
      '/clear',
      '/stop',
      '/cancel',
      '/status',
      '/commands',
      '/help',
      '/skills',
      '/tools',
      '/pending',
      '/approvals',
      '/tasks',
      '/todo',
      '/resume',
    ]) {
      expect(isControlCommand(command)).toBe(true);
      expect(isControlCommand(`  ${command.toUpperCase()}  `)).toBe(true);
    }
  });

  it('keeps prefix lookalikes as normal chat messages', () => {
    for (const text of ['/clearly not', '/statuses', '/tools please list', 'hello /clear']) {
      expect(isControlCommand(text)).toBe(false);
    }
  });

  it('handles the optional approval id and the workflow trigger', () => {
    expect(isControlCommand('/approve 3f2a1b')).toBe(true);
    expect(isControlCommand('/reject')).toBe(true);
    expect(isControlCommand('/workflow write the release notes')).toBe(true);
  });

  it('only stop/cancel drop the running state', () => {
    expect(endsTurn('/stop')).toBe(true);
    expect(endsTurn('/CANCEL')).toBe(true);
    expect(endsTurn('/clear')).toBe(false);
    expect(endsTurn('/stopwatch')).toBe(false);
    expect(endsTurn('please stop')).toBe(false);
  });
});
