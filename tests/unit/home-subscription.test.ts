import {invoke} from '@tauri-apps/api/core';
import {beforeEach, expect, it, vi} from 'vitest';
import {subscribeHome} from '../../src/app/backend';
import type {HomeSnapshot} from '../../src/app/contracts';
import {referenceSnapshot} from '../../src/testing/fixtures';

const events = vi.hoisted(() => ({home: undefined as ((event: {payload: HomeSnapshot}) => void) | undefined}));
vi.mock('@tauri-apps/api/core', () => ({invoke: vi.fn()}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (_name: string, listener: typeof events.home) => {
    events.home = listener;
    return () => { events.home = undefined; };
  },
}));

beforeEach(() => { vi.mocked(invoke).mockReset(); });

const snapshot = (revision: number): HomeSnapshot => ({...referenceSnapshot, revision});

it('applies the newest complete Home event across gaps and ignores later stale events', async () => {
  vi.mocked(invoke).mockResolvedValueOnce(snapshot(1)).mockRejectedValue(new Error('Home read unavailable'));
  const revisions: number[] = [];
  const stop = await subscribeHome(value => revisions.push(value.revision));
  try {
    for (const revision of [3, 5, 4, 6]) events.home?.({payload: snapshot(revision)});
    expect(revisions).toEqual([1, 3, 5, 6]);
  } finally {
    stop();
  }
});

it('keeps a newer event received while the initial snapshot is pending', async () => {
  let finish!: (value: HomeSnapshot) => void;
  vi.mocked(invoke).mockReturnValueOnce(new Promise<HomeSnapshot>(resolve => { finish = resolve; })).mockResolvedValue(snapshot(2));
  const revisions: number[] = [];
  const subscribing = subscribeHome(value => revisions.push(value.revision));
  await Promise.resolve();
  events.home?.({payload: snapshot(7)});
  finish(snapshot(2));
  const stop = await subscribing;
  stop();
  expect(revisions.at(-1)).toBe(7);
  expect(revisions.every((revision, index) => index === 0 || revisions[index - 1] < revision)).toBe(true);
});
