import { fail, json } from '@sveltejs/kit';
import { message, superValidate, withFiles } from 'sveltekit-superforms/server';
import { searchFormSchema } from './schema';
import type { Actions, PageServerLoad } from './$types';
import { valibot } from 'sveltekit-superforms/adapters';

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

export const load = (async () => {
	return {
		form: await superValidate(valibot(searchFormSchema))
	};
}) satisfies PageServerLoad;

export const actions = {
	default: async ({ request }) => {
		const form = await superValidate(request, valibot(searchFormSchema));
		if (!form.valid) return fail(400, { form });

		const file = form.data.image;
		if (!(file instanceof File)) {
			return fail(400, { form, error: 'No image provided' });
		}

		try {
			const apiFormData = new FormData();
			apiFormData.append('image', file);

			const response = await fetch('http://localhost:3000/search', {
				method: 'POST',
				body: apiFormData
			});

			const responseText = await response.text();
			console.log('Raw API response:', responseText);

			if (!response.ok) {
				return fail(response.status, { form, error: `Search API error: ${response.status}` });
			}

			const results = JSON.parse(responseText);
			console.log('Parsed Results:', results);

			/*	return {
				form: {
					...form,
					data: {
						...form.data,
						image: undefined,
						results: [
							{
								msg_id: 1,
								chat_id: 1,
								posted_at: '2023-01-01T00:00:00.000Z',
								similarity: 0.5,
								display_name: 'Test User',
								user_name: null,
								bias: undefined,
								invite_hash: null,
								tags: []
							}
						]
					}
				},
				success: true
			}; */

			/*	return {
				form: {
					...form,
					data: {
						...form.data,
						image: undefined
					}
				},
				success: true,
				results: [
					{
						msg_id: 1,
						chat_id: 1,
						posted_at: '2023-01-01T00:00:00.000Z',
						similarity: 0.5,
						display_name: 'Test User',
						user_name: null,
						bias: undefined,
						invite_hash: null,
						tags: []
					}
				]
			}; */

			return withFiles({
				form: { ...form, data: { ...form.data, results: results } },
				results: [
					{
						msg_id: 1,
						chat_id: 1,
						posted_at: '2023-01-01T00:00:00.000Z',
						similarity: 0.5,
						display_name: 'Test User',
						user_name: null,
						bias: undefined,
						invite_hash: null,
						tags: []
					}
				]
			});
		} catch (error) {
			console.error('Upload error:', error);
			return fail(500, { form, error: error instanceof Error ? error.message : 'Upload failed' });
		}
	}
} satisfies Actions;
