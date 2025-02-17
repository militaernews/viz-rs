// src/lib/schemas.ts
import { file, maxSize, mimeType, object, pipe, type InferInput } from 'valibot';

export const authorizedExtensions = ['image/jpeg', 'image/png'];

export const searchFormSchema = object({
	image: pipe(
		file('Please select an image file.'),
		mimeType(['image/jpeg', 'image/png'], 'Please select a JPEG or PNG file.'),
		maxSize(1024 * 1024 * 10, 'Please select a file smaller than 10 MB.')
	)
});

export type SearchFormSchema = InferInput<typeof searchFormSchema>;
