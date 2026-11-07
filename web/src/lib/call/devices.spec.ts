import { describe, expect, it } from 'vitest';
import { describeDevices, deviceOf } from './devices';

const info = (kind: MediaDeviceKind, deviceId: string, label = '') => ({ kind, deviceId, label });

describe('describeDevices', () => {
	it('sorts devices into the three lists and keeps their names', () => {
		const lists = describeDevices([
			info('audioinput', 'm1', 'USB microphone'),
			info('videoinput', 'c1', 'Laptop camera'),
			info('audiooutput', 's1', 'Headphones')
		]);
		expect(lists.audioIn).toEqual([{ id: 'm1', label: 'USB microphone' }]);
		expect(lists.videoIn).toEqual([{ id: 'c1', label: 'Laptop camera' }]);
		expect(lists.audioOut).toEqual([{ id: 's1', label: 'Headphones' }]);
	});

	it('names devices whose label the browser hides, and skips the duplicate "communications" entry', () => {
		const lists = describeDevices([info('audioinput', 'a'), info('audioinput', 'communications', 'dup'), info('audioinput', 'b'), info('videoinput', 'c')]);
		expect(lists.audioIn.map((d) => d.label)).toEqual(['Microphone 1', 'Microphone 2']);
		expect(lists.videoIn[0].label).toBe('Camera 1');
	});

	it('reads the device of a track', () => {
		expect(deviceOf({ getSettings: () => ({ deviceId: 'x' }) } as unknown as MediaStreamTrack)).toBe('x');
		expect(deviceOf(undefined)).toBe('');
	});
});
