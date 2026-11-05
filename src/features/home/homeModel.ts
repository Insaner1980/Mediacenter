import type {ServiceId} from '../../app/contracts';
export type {Theme,ServiceId,ServiceDefinition,HomeSnapshot} from '../../app/contracts';
export type Screen = 'home' | 'video' | 'audio' | 'photo' | 'drives' | 'settings';
export type HomeAction = { type: 'navigate'; screen: Screen } | { type: 'service'; serviceId: ServiceId };
