import {UserVo} from "@/vo/user.vo";

export interface ProfileVo {
    name?: string
    avatar?: string
    description?: string
    user: UserVo
}
