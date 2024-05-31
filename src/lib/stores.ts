import { get, writable } from 'svelte/store';
import { invokeCommand } from './invoke';
import type { FiltersResponse, Game, GameInfo, ProfileInfo, ProfilesInfo } from './models';
import { fetch } from '@tauri-apps/api/http';

export let games: Game[] = [];
export let categories: string[] = [];
export const currentGame = writable<Game | undefined>(undefined);

export let activeProfileIndex: number = 0;
export let profiles: ProfileInfo[] = [];
export const currentProfile = writable<ProfileInfo>({
	name: '',
	modCount: 0
});

refreshGames();

export async function refreshGames() {
	const info: GameInfo = await invokeCommand('get_game_info');
	games = info.all;

	for (let game of games) {
		game.favorite = info.favorites.includes(game.id);
	}

	currentGame.set(info.active);
	refreshProfiles();
	refreshCategories();
}

export async function setActiveGame(game: Game) {
	await invokeCommand('set_active_game', { id: game.id });
	refreshGames();
}

export async function refreshCategories() {
	let gameId = get(currentGame)?.id;
	if (!gameId) return;

	let response = await fetch<FiltersResponse>(`https://thunderstore.io/api/cyberstorm/community/${gameId}/filters/`);
	categories = response.data.package_categories.map(c => c.name);
}

export async function refreshProfiles() {
	const info = await invokeCommand<ProfilesInfo>('get_profile_info');
	activeProfileIndex = info.activeIndex;
	profiles = info.profiles;
	currentProfile.set(profiles[activeProfileIndex]);
}

export async function setActiveProfile(index: number) {
	await invokeCommand('set_active_profile', { index });
	refreshProfiles();
}
