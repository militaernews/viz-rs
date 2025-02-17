export const formatDate = (dateString: string) => {
	return new Date(dateString).toLocaleTimeString([], {
		year: 'numeric',
		month: 'numeric',
		day: 'numeric',
		hour: '2-digit',
		minute: '2-digit',
		hour12: false
	});
};
