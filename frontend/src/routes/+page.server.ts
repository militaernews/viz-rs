import { fail } from '@sveltejs/kit';
import type { Actions } from './$types';

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

export const actions = {
	default: async ({ request }) => {
		const formData = await request.formData();
		const image = formData.get('image') as File;

		if (!image) {
			return fail(400, {
				error: 'No image provided'
			});
		}

		try {
			const body = new FormData();
			body.append('image', image);

			console.log("posting")

			const response = await fetch('http://app:3000/search', {
				method: 'POST',
				body
			});

			console.log("response" + JSON.stringify(response))

			if (!response.ok) {
				return fail(400, {
					error: 'Failed to upload image'
				});
			}

			const data = await response.json();

			return {
				success: image.name,
				data
			};
		} catch (error) {
			return fail(500, {
				error: error instanceof Error ? error.message : 'Failed to upload image'
			});
		}
	}
} satisfies Actions;
