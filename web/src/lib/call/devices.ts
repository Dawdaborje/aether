/** The microphones, cameras and speakers a person can pick from, with a readable name for each. */

export interface Device {
	id: string;
	label: string;
}

export interface DeviceLists {
	audioIn: Device[];
	videoIn: Device[];
	audioOut: Device[];
}

/** Browsers hide the names until the person allowed the devices once; then "Microphone 2" does as a name. */
export function describeDevices(devices: readonly Pick<MediaDeviceInfo, 'kind' | 'deviceId' | 'label'>[]): DeviceLists {
	const out: DeviceLists = { audioIn: [], videoIn: [], audioOut: [] };
	const names = { audioinput: 'Microphone', videoinput: 'Camera', audiooutput: 'Speaker' } as const;
	const lists = { audioinput: out.audioIn, videoinput: out.videoIn, audiooutput: out.audioOut } as const;
	for (const device of devices) {
		const list = lists[device.kind as keyof typeof lists];
		// `default` and `communications` repeat another entry under another name in Chromium.
		if (!list || device.deviceId === 'communications') continue;
		list.push({ id: device.deviceId, label: device.label || `${names[device.kind as keyof typeof names]} ${list.length + 1}` });
	}
	return out;
}

/** The device a track comes from, to mark the current one in a list. */
export function deviceOf(track: MediaStreamTrack | undefined): string {
	return track?.getSettings?.().deviceId ?? '';
}
