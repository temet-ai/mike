export type SessionFlowSource = {
	table: string;
	row_id: string;
	session_id: string | null;
	request_id: string | null;
	detail_href: string | null;
};
