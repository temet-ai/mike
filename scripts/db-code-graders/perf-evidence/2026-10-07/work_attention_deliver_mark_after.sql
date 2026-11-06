UPDATE agents.work_item_attention a
   SET delivered_message_uuid = 'fc3bb8a9-b27b-4477-bb36-14a8b558573a'
 WHERE a.id = 2293649
   AND EXISTS (SELECT 1 FROM mailbox.messages m WHERE m.message_uuid = 'fc3bb8a9-b27b-4477-bb36-14a8b558573a')
