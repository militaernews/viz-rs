export interface SearchResult {
	msg_id: number;
	chat_id: number;
	posted_at: string;
	similarity: number;
	display_name: string;
	user_name: string | null;
	bias: string | undefined;
	invite_hash: string | null;
	tags: string[];
}
