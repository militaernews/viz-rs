import type { SearchResult } from './SearchResult';

export interface SearchHistoryEntry {
	id: string;
	timestamp: number;
	results: SearchResult[];
	searchParams: {
		tags: string[];
		startDate: string;
		endDate: string;
		imageFileName?: string;
		videoFileName?: string;
	};
}
