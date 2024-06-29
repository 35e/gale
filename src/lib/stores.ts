import { get, writable } from 'svelte/store';
import { invokeCommand } from './invoke';
import {
	SortBy,
	SortOrder,
	type FiltersResponse,
	type Game,
	type GameInfo,
	type PackageCategory,
	type ProfileInfo,
	type ProfilesInfo,
	type QueryModsArgs
} from './models';
import { fetch } from '@tauri-apps/api/http';

export let games: Game[] = [];
export let categories = writable<PackageCategory[]>([]);
export let activeGame = writable<Game | null>(null);

export let activeProfileIndex: number = 0;
export let profiles: ProfileInfo[] = [];
export let activeProfile = writable<ProfileInfo | null>(null);

refreshGames();

function loadQuery(key: string, getDefault: () => QueryModsArgs) {
	let json = localStorage.getItem(key);
	console.log('Loaded query:', json);
	if (json) {
		try {
			return JSON.parse(json);
		} catch (e) {
			console.error('Failed to parse stored query:', e);
		}
	}

	return getDefault();
}

function createQueryStore(key: string, getDefault: () => QueryModsArgs) {
	let store = writable<QueryModsArgs>(loadQuery(key, getDefault));
	store.subscribe((value) => {
		localStorage.setItem(key, JSON.stringify(value));
	});
	return store;
}

const defaultModQuery = () => ({
	maxCount: 20,
	searchTerm: '',
	includeCategories: [],
	excludeCategories: [],
	includeNsfw: false,
	includeDeprecated: false,
	includeDisabled: false,
	sortBy: SortBy.LastUpdated,
	sortOrder: SortOrder.Descending
});

const defaultProfileQuery = () => ({
	maxCount: 20,
	searchTerm: '',
	includeCategories: [],
	excludeCategories: [],
	includeNsfw: true,
	includeDeprecated: true,
	includeDisabled: true,
	sortBy: SortBy.Custom,
	sortOrder: SortOrder.Descending
});

export let modQuery = createQueryStore('modQuery', defaultModQuery);
export let profileQuery = createQueryStore('profileQuery', defaultProfileQuery);

activeGame.subscribe(() => {
	modQuery.set(defaultModQuery());
	profileQuery.set(defaultProfileQuery());
});

export async function refreshGames() {
	const info: GameInfo = await invokeCommand('get_game_info');
	games = info.all;

	for (let game of games) {
		game.favorite = info.favorites.includes(game.id);
	}

	activeGame.set(info.active);
	refreshProfiles();
	refreshCategories();
}

export async function setActiveGame(game: Game) {
	await invokeCommand('set_active_game', { id: game.id });
	refreshGames();
}

export async function refreshCategories() {
	let gameId = get(activeGame)?.id;
	if (!gameId) return;

	let response = await fetch<FiltersResponse>(
		`https://thunderstore.io/api/cyberstorm/community/${gameId}/filters/`
	);
	categories.set(response.data.package_categories);
}

export async function refreshProfiles() {
	let info = await invokeCommand<ProfilesInfo>('get_profile_info');

	activeProfileIndex = info.activeIndex;
	profiles = info.profiles;
	activeProfile.set(profiles[activeProfileIndex]);
}

export async function setActiveProfile(index: number) {
	await invokeCommand('set_active_profile', { index });
	refreshProfiles();
}
