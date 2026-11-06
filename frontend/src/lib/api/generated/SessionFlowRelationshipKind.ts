export type SessionFlowRelationshipKind =
	| 'dispatch_tool'
	| 'result_carrier'
	| 'message_sender'
	| 'message_recipient'
	| 'message_reply'
	| 'explicit_parent'
	| 'request_tool'
	| 'activity_request';
